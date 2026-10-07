//! Batterie 2 — désérialisation de structures malformées et aller-retour disque.

use std::io::Write;
use std::path::PathBuf;
use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, MixedCell, Project, ValueBlock,
};
use stokesforge_persistence::{load, save};
use tempfile::TempDir;

const MAGIC: &[u8; 4] = b"STKF";

fn ecrire_fichier(dir: &TempDir, nom: &str, corps: &[u8], version: u32) -> PathBuf {
    let path = dir.path().join(nom);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(MAGIC).unwrap();
    f.write_all(&version.to_le_bytes()).unwrap();
    f.write_all(corps).unwrap();
    f.sync_all().unwrap();
    path
}

#[test]
fn b2_corps_aleatoires_ne_paniquent_jamais() {
    let dir = TempDir::new().unwrap();
    // Graines déterministes : xorshift maison, aucune dépendance.
    let mut etat: u64 = 0x2026_0911_2026_0911;
    let mut suivant = move || {
        etat ^= etat << 13;
        etat ^= etat >> 7;
        etat ^= etat << 17;
        etat
    };
    for cas in 0..2000u32 {
        let taille = (suivant() % 512) as usize;
        let corps: Vec<u8> = (0..taille).map(|_| (suivant() & 0xff) as u8).collect();
        let path = ecrire_fichier(&dir, &format!("f{cas}.stf"), &corps, 1);
        let r = std::panic::catch_unwind(|| load(&path));
        assert!(
            r.is_ok(),
            "panique sur un corps aléatoire de {taille} octets : {corps:02x?}"
        );
        if let Ok(Ok(p)) = r {
            // Un corps aléatoire qui passe : on veut le savoir.
            println!(
                "cas {cas} ACCEPTÉ ({} nœuds) corps={corps:02x?}",
                p.tree().len()
            );
        }
    }
}

#[test]
fn b2_troncature_progressive_dun_fichier_valide() {
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("Tronque").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let complet = dir.path().join("complet.stf");
    save(&project, &complet).unwrap();
    let octets = std::fs::read(&complet).unwrap();

    for n in 0..octets.len() {
        let path = dir.path().join(format!("t{n}.stf"));
        std::fs::write(&path, &octets[..n]).unwrap();
        let r = std::panic::catch_unwind(|| load(&path));
        assert!(r.is_ok(), "panique sur une troncature à {n} octets");
        if let Ok(Ok(_)) = r {
            panic!(
                "une troncature à {n}/{} octets a été ACCEPTÉE",
                octets.len()
            );
        }
    }
}

#[test]
fn b2_mutation_dun_octet_dans_un_fichier_valide() {
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("Bitflip").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    project
        .add_graph(root, Some("G1"), stokesforge_plot::PlotSpec::empty("t"))
        .unwrap();
    let complet = dir.path().join("complet.stf");
    save(&project, &complet).unwrap();
    let octets = std::fs::read(&complet).unwrap();

    let mut acceptes = 0usize;
    for i in 8..octets.len() {
        for bit in [0x01u8, 0x80u8] {
            let mut copie = octets.clone();
            copie[i] ^= bit;
            let path = dir.path().join("mut.stf");
            std::fs::write(&path, &copie).unwrap();
            let r = std::panic::catch_unwind(|| load(&path));
            assert!(
                r.is_ok(),
                "panique sur un bit retourné en position {i} (masque {bit:#04x})"
            );
            if let Ok(Ok(_)) = r {
                acceptes += 1;
            }
        }
    }
    println!(
        "{acceptes} variantes d'un bit acceptées sur {} essais (attendu : celles qui ne \
         touchent que des données utiles)",
        (octets.len() - 8) * 2
    );
}

#[test]
fn b2_aller_retour_preserve_tout() {
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("AR").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();

    // Colonne numérique aux valeurs pathologiques.
    let pathologiques = [
        0.0,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MIN_POSITIVE,
        1e-320,
        f64::MAX,
        f64::MIN,
    ];
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&pathologiques))
        .unwrap();
    // Un NaN de charge utile non standard : le format promet « bit à bit ».
    let nan_signalant = f64::from_bits(0x7ff4_0000_dead_beef);
    project
        .set_values(a, 0, ValueBlock::Float(vec![nan_signalant]))
        .unwrap();

    // Colonne texte (mixte, entièrement textuelle) avec vide / None / unicode
    // / caractère nul.
    let mut textes = Column::new(ColumnType::Mixed);
    textes
        .set_values(
            0,
            ValueBlock::Mixed(vec![
                MixedCell::Text(String::new()),
                MixedCell::Empty,
                MixedCell::Text("é\u{0}à".into()),
                MixedCell::Text("🦀".into()),
                MixedCell::Text("  espaces  ".into()),
            ]),
        )
        .unwrap();
    let b = project.add_column(sheet, None, Some("B"), textes).unwrap();
    project
        .set_column_meta(
            b,
            ColumnMeta {
                role: ColumnRole::XError,
                unit: "µm".into(),
                long_name: "Nom long".into(),
                comment: "".into(),
                x_prime: Some(-0.5),
                display_format: "%.3f".into(),
                formula: None,
                file_metadata: Vec::new(),
                import_source: None,
            },
        )
        .unwrap();

    // Colonne à types mixtes : nombre, texte, cellule vide, NaN écrit comme
    // un nombre délibéré (distinct de la cellule vide), texte unicode.
    let mixte = Column::from_mixed(&[
        MixedCell::Number(1.5),
        MixedCell::Text("n/a".to_string()),
        MixedCell::Empty,
        MixedCell::Number(f64::NAN),
        MixedCell::Text(String::new()),
        MixedCell::Text("é ü 日本".to_string()),
    ]);
    project.add_column(sheet, None, Some("C"), mixte).unwrap();

    let path = dir.path().join("ar.stf");
    save(&project, &path).unwrap();
    let relu = load(&path).unwrap();

    assert_eq!(project.tree().len(), relu.tree().len());
    assert_eq!(project.next_id_hint(), relu.next_id_hint());

    for (id, node) in project.tree().iter() {
        let autre = relu.tree().node(id).expect("nœud perdu au rechargement");
        assert_eq!(node.name(), autre.name());
        assert_eq!(node.parent(), autre.parent());
        if let (Some(c1), Some(c2)) = (node.as_column(), autre.as_column()) {
            assert_eq!(c1.column_type(), c2.column_type());
            assert_eq!(c1.meta(), c2.meta());
            assert_eq!(
                c1.version(),
                c2.version(),
                "compteur de version perdu pour {}",
                node.name()
            );
            assert_eq!(c1.len(), c2.len());
            for row in 0..c1.len() {
                let (x, y) = (c1.get(row), c2.get(row));
                assert_eq!(x, y, "colonne {} ligne {row} : {x:?} != {y:?}", node.name());
            }
        }
    }
    // Déterminisme octet à octet promis par la doc de `save`.
    let path2 = dir.path().join("ar2.stf");
    save(&relu, &path2).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        std::fs::read(&path2).unwrap(),
        "deux sauvegardes du même projet diffèrent"
    );
}

#[test]
fn b2_projet_enorme_en_largeur_se_recharge() {
    // 2000 colonnes dans une feuille : la validation au chargement est en
    // O(n²) sur les enfants d'un même parent (comptage d'occurrences).
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("Large").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    for i in 0..2000 {
        project
            .add_column(
                sheet,
                None,
                Some(&format!("C{i}")),
                Column::from_f64(&[1.0]),
            )
            .unwrap();
    }
    let path = dir.path().join("large.stf");
    save(&project, &path).unwrap();
    let debut = std::time::Instant::now();
    let relu = load(&path).unwrap();
    let duree = debut.elapsed();
    println!("rechargement de 2000 colonnes : {duree:?}");
    assert_eq!(relu.tree().len(), project.tree().len());
    assert!(
        duree < std::time::Duration::from_secs(10),
        "rechargement anormalement long : {duree:?}"
    );
}

#[test]
fn b2_profondeur_de_dossiers_extreme() {
    // Arbre très profond : le chargement valide par un parcours ; s'il était
    // récursif, la pile sauterait.
    let dir = TempDir::new().unwrap();
    let mut project = Project::new("Profond").unwrap();
    let mut parent = project.root();
    for i in 0..5000 {
        parent = project.add_folder(parent, &format!("d{i}")).unwrap();
    }
    let path = dir.path().join("profond.stf");
    save(&project, &path).unwrap();
    let relu = load(&path).unwrap();
    assert_eq!(relu.tree().len(), 5001);
    // Et le chemin canonique du plus profond.
    let chemin = relu.path_of(parent).unwrap();
    assert!(chemin.ends_with("/d4999"));
}
