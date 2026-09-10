# Options explorées et écartées

Compagnon d'`architecture-choisie.md`. Règle du dossier : on ne supprime pas
une option rejetée, on garde **pourquoi**.

---

## Graphiques 2D/3D

Il n'existe **pas de vrai portage** de matplotlib vers Rust (traduction du
code Python) — seulement des réimplémentations pure-Rust avec API inspirée,
ou des wrappers qui appellent Python en subprocess.

| Crate | Type | Verdict |
|---|---|---|
| **plotkit** | Pure Rust, API façon matplotlib (`Figure`/`Axes`), downsampling automatique (LTTB), anti-aliasing natif, export PNG/SVG/PDF | Écarté au profit de Plotters : plutôt orienté génération statique, moins mature pour l'interaction temps réel. **Coût assumé de ce rejet** : le LTTB est à réécrire nous-mêmes si la tessellation coince (`architecture-choisie.md` §4.2) |
| **Plotters** | Pure Rust, bas niveau (`ChartBuilder`), pas d'anti-aliasing natif | **Retenu** — jugé non bloquant car export final vectoriel ; trait `DrawingBackend` à surface minuscule, donc backend egui écrit par nous (voir plus bas) |
| PlottersSkill | Binding JS/TS de `plotters` | Non pertinent (hors Rust natif) |
| matplotlib / rust-plotter / plotpy | Wrappers appelant Python **en sous-processus** | Écartés — un aller-retour de sous-processus par tracé, et le rendu sort d'un espace d'adressage étranger |

> **Correction du 2026-09-10 — ne pas rejouer cet argument.** Une version
> antérieure de cette table écartait ces wrappers parce qu'ils
> « réintroduisent le problème Python/undo-redo ». L'argument est juste
> pour un **sous-processus**, et faux pour PlotYx : l'interpréteur du socle
> est **dans le processus** (PyO3), et les écritures d'un script passent
> par le même bus de commandes que l'UI, donc par la même pile d'annulation
> — testé (`crates/plotyx-script/tests/script.rs:552`). Le motif de rejet
> est la latence et la frontière d'adressage, **pas** l'annulation. Détail
> complet en `architecture-choisie.md` §2.1.

### Le renderer Matplotlib embarqué (plan du 2026-09-08) — écarté le 2026-09-10

Le plan antérieur (spec `rustrewrite` §12) rendait par **Matplotlib dans
l'interpréteur PyO3**, sortie `FigureCanvasAgg.buffer_rgba()` uploadée
comme texture egui. Écarté au profit d'un rendu Rust natif.

**Ce qu'on perd, et c'est la vraie facture de la bascule** : Matplotlib
gratuit — échelles log, placement des ticks, légendes, export PDF/SVG
mature, et ~30 ans de cas limites scientifiques. Plotters est bas niveau.
Cette perte est le coût principal du changement ; il est assumé, pas nié.

**Ce qu'on gagne** :

- le chemin de redraw devient purement Rust (`Arc<Tree>` → Plotters), plus
  court que l'aller-retour Python + upload de texture ;
- les **deux points de vigilance** du plan Matplotlib disparaissent avec
  lui : le budget de 141 ms et la mise à jour **partielle** de texture pour
  tenir le réticule à 0,2 ms n'ont plus d'objet, faute de texture ;
- moins cher en mémoire : cache vectoriel **2,4 Mo** contre **7,7 Mo** de
  texture pour 500 spectres de 600 points, et indépendant de la résolution
  (`architecture-choisie.md` §4.2).

**Effet de bord sur le zéro-copie (spec §8.3)** — il avait deux
consommateurs, Matplotlib et les scripts ; il n'en garde qu'un. **La
décision ne bouge pas** : la revue du 2026-09-08 avait déjà acté que le
zéro-copie « ne se justifie pas par le temps mais par la mémoire », et il
reste contractuel (`cahier-des-charges-api-script.md` §2.3 : une colonne
lue renvoie une vue). Rien à défaire : c'est construit et testé.

---

## Backend egui pour Plotters

| Solution | Verdict |
|---|---|
| **`egui-plotter` 0.6.0** | Écarté — dépend d'`egui` **0.32.1**, or `plotyx-app` est sur **0.36.2** |
| **`egui-plotter-fork` 0.35.0** | Écarté — dépend d'`egui` **0.35**, toujours en retard ; figer egui à 0.35 serait une régression sur une machine où un défaut Wayland de `winit`/`eframe` est déjà documenté (spec §9) |
| **Vendoriser l'un des deux** | Écarté — ~1 000 lignes à maintenir (`backend.rs` 553 + `chart.rs` 375), dont ~208 de newtypes à surcharges d'opérateurs et ~70 de `bg_image` inutile (« pas de gestion d'images » au cahier des charges) |
| **Backend écrit par nous** | **Retenu** — le trait n'exige que 4 méthodes, 8 surchargées pour du vectoriel, ~250-300 lignes pour nos besoins. Détail : `architecture-choisie.md` §4 |

**Critère transverse appliqué ici** : mono-mainteneur + en retard d'une
version d'egui, c'est le profil qui a fait écarter `latex-rust` et ReX plus
bas. On l'applique aussi à `egui-plotter`, par cohérence.

`chart.rs` reste une **référence à lire** (`MouseConfig`, transformation 3D)
avant de réécrire l'interaction souris — c'est le morceau qui a de la valeur
dans ce crate.

### Écrire dans un `Painter` plutôt que dans un `Vec<Shape>`

Écarté. C'est le choix d'`egui-plotter`, et il oblige à rejouer tout le
pipeline Plotters à **chaque frame** — egui est en mode immédiat et repeint
à chaque mouvement de souris. Un collecteur de `Shape` permet un cache dont
`epaint` documente lui-même la validité (« storing them should also be
fine »), avec pour seule exception `Shape::Text`, lié à `pixels_per_point`
— d'où sa présence dans la clé de cache. Détail : §4.2.

### Cacher la tessellation (`Shape::Mesh(Arc<Mesh>)`)

Écarté **comme défaut**, gardé comme bouton. ~14 Mo pour 300 k points
(600 k `Vertex` à 20 o + indices `u32`) : plus cher que la texture qu'on
vient de refuser. À n'activer que si la mesure du point ouvert n°1 le
justifie, et après le LTTB.

---

## Undo/Redo

| Solution | Verdict |
|---|---|
| **crate `undo` 0.52** | Écarté — ne connaît pas la distinction **instantané de structure / delta de données** ni le LIFO strict qui en découle. L'adopter serait une régression |
| **`plotyx-commands::History`** | **Retenu — déjà construit** : 1 006 lignes, 3 187 lignes de tests (dont proptest). Historique hybride `Arc<Tree>` pour la structure, delta de cellules pour les données. Spec `rustrewrite` §5 |

---

## Performance grande échelle (GPU)

Pistes explorées pour ~300k points, finalement jugées inutiles :

- **`egui_wgpu_plot`** — transform data→écran sur GPU, buffers non
  ré-uploadés au pan/zoom.
- **`runmat-plot`** — wgpu complet, mais pré-1.0 et export encore marqué
  TODO.

Écartées : la contrainte de fluidité a été relâchée (bon jusqu'à 100
courbes, ralentissement accepté jusqu'à 500), donc **Plotters + backend egui
en CPU suffit**, pas besoin de complexifier avec du GPU custom.

---

## Fenêtres flottantes

- **`egui_tool_windows`** — panneaux flottants intra-fenêtre,
  redimensionnables/draggables, mais contraints au conteneur parent.
- **Viewports natifs egui** — vraies fenêtres OS indépendantes,
  multi-moniteur. **Retenu** pour un rendu type OriginLab
  (`egui_tool_windows` gardé en option pour des panneaux internes).

---

## Rendu de notations mathématiques / texte riche

Après hésitation entre deux approches (une dédiée aux labels de plot
légers, une pour le texte riche/documents), **Typst embarqué a été retenu
comme solution unique** pour l'ensemble des besoins de rendu math/texte
riche — mais **uniquement pour les notations** : le texte ordinaire (ticks,
noms de série) passe par les fonts egui, une compilation Typst par tick
étant hors de question (`architecture-choisie.md` §4.3).

| Solution | Description | Verdict |
|---|---|---|
| Maison minimal | 2 `painter.text()` avec offset pour indices/exposants | Écarté — suffit pour cas simples, mais recode un mini-moteur dès qu'il faut des fractions propres |
| **pulldown-latex** | Parseur LaTeX → MathML | Écarté — nécessite un moteur de rendu MathML supplémentaire (webview), pas adapté ici |
| **katex-rs** | Portage Rust de KaTeX → HTML/MathML | Écarté malgré une vraie adoption (3766 téléchargements/mois, 10 crates dépendantes) — pensé pour rendu web (HTML/CSS), mauvais fit pour export SVG/PDF natif |
| **latex-rust** | Parseur LaTeX pure Rust → SVG/PNG/shapes egui direct | Écarté au profit de Typst embarqué — avait été envisagé un temps pour les labels de plot (sortie directe en shapes egui sans étape intermédiaire), mais jeune (v1.0, 2026) et mono-mainteneur |
| **ReX** | Moteur de rendu maths pur Rust, inspiré TeX/KaTeX | Écarté — abandonné (dernier commit ancien), jamais publié sur crates.io, README déconseille l'usage en prod |
| **Tectonic** | Moteur TeX/LaTeX complet (fork XeTeX), embeddable | Écarté — overkill, dépend de TeXLive/Harfbuzz externe, complexité de build disproportionnée pour de simples snippets maths |
| **`typst-pack`** | Empaquette un projet Typst (fonts + sources) en blob autonome (`PackWorld`) | Écarté pour l'instant — trop jeune, API mouvante ; à garder en veille si besoin de gestion de templates/packages plus riche |
| **`typst-as-lib`** | Wrapper de `typst` pensé « templating de document complet » | Écarté au profit d'un `World` custom — moins adapté au rendu de fragments unitaires, API qui bouge plus vite (0.14 → 0.16), plus d'overhead par compilation |

### Fonts Typst envisagées

- **Set 1** : Libertinus Serif + Libertinus Math (SIL OFL 1.1) — cohérence
  visuelle texte/maths maximale.
- **Set 2** : Liberation Sans + Noto Sans Math (SIL OFL) — look « Arial »
  (Arial elle-même propriétaire, sans compagnon maths officiel).
  - Alternative envisagée dans ce set : **Fira Math** (OFL, plus large
    couverture de glyphes) — écartée au profit de Noto Sans Math pour la
    cohérence avec l'écosystème Noto (utile si extension CJK future).

---

## Export PDF

| Solution | Verdict |
|---|---|
| Backend PDF de Plotters | **N'existe pas** — Plotters ne sort que bitmap et SVG |
| **`svg2pdf` 0.13.0** | **Retenu** — `PlotSpec → SVGBackend → svg2pdf`. Vient de l'organisation Typst, donc de la famille déjà adoptée |
| `typst-pdf` | Gardé en veille — pertinent le jour où une figure serait composée *dans* un document Typst, pas pour une figure seule |
| Export par sous-processus Python | Toujours pertinent pour un export lourd et ponctuel (spec §12) — la raison du rejet côté console (latence par appel) ne s'applique pas à une opération unique |
