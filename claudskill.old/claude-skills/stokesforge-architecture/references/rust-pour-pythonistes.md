# Rust expliqué à quelqu'un qui connaît Python

Table de traduction à utiliser **quand un terme Rust est inévitable** dans une
explication. Chaque entrée donne l'analogie Python la plus proche **et** ce
qu'elle a de faux — une analogie non corrigée finit toujours par produire une
mauvaise attente.

Ne pas dérouler cette table dans une réponse. Y piocher le mot dont on a
besoin, au moment où on en a besoin, en une phrase.

## Organisation du code

| Rust | En Python | Où l'analogie casse |
|---|---|---|
| *crate* | un paquet installable (`numpy`) | un crate est aussi l'unité de compilation : les frontières entre crates sont vérifiées par le compilateur, pas par convention |
| *workspace* | un dépôt contenant plusieurs paquets | ils partagent un seul fichier de versions (`Cargo.lock`) et se compilent ensemble |
| `Cargo.toml` | `pyproject.toml` | — |
| `cargo` | `pip` + `pytest` + `make` réunis | — |
| module, `mod` | un fichier `.py` | — |
| `pub` | pas de `_` devant le nom | en Rust c'est une vraie barrière : sans `pub`, c'est inaccessible, pas seulement déconseillé |
| *feature* | un `extra` de `pip install paquet[extra]` | une feature non activée n'est **pas compilée du tout** : le code derrière elle n'est ni vérifié ni testé |

## Types

| Rust | En Python | Où l'analogie casse |
|---|---|---|
| `struct` | une `dataclass` | — |
| `enum` | un `Enum`… ou plutôt une `Union` de dataclasses | un `enum` Rust peut porter des données différentes dans chaque variante ; c'est son usage principal ici |
| `trait` | un `Protocol` / du *duck typing* nommé | le compilateur vérifie l'implémentation ; rien n'est résolu à l'exécution |
| `Option<T>` | `Optional[T]` | on ne peut pas oublier de traiter le cas vide : le compilateur refuse |
| `Result<T, E>` | une valeur ou une exception | l'erreur est une **valeur rendue**, pas un saut ; elle doit être traitée explicitement |
| `match` | un `match` Python, en plus strict | il faut couvrir tous les cas, sinon ça ne compile pas |
| générique (`fn f<T>`) | une fonction qui accepte n'importe quel type | le code est recopié et spécialisé pour chaque type à la compilation (*monomorphisation*) — d'où le souci, dans le rendu, de ne pas multiplier ces copies |

## Mémoire et partage

| Rust | En Python | Où l'analogie casse |
|---|---|---|
| *ownership* (propriété) | rien d'équivalent | une valeur a un seul propriétaire ; quand il disparaît, elle est libérée. Pas de ramasse-miettes |
| `&T` (emprunt partagé) | passer un objet à une fonction qui le lit | on peut en avoir autant qu'on veut, mais rien ne peut modifier pendant ce temps |
| `&mut T` (emprunt exclusif) | passer un objet à une fonction qui le modifie | il ne peut y en avoir **qu'un seul à la fois**, vérifié à la compilation. C'est la clé de l'architecture de StokesForge |
| `Arc<T>` | une référence Python ordinaire (comptée) | partage **en lecture seule** entre plusieurs fils ; pour modifier, il faut un verrou |
| `clone()` | `copy.copy()` | souvent bon marché ici : cloner un `Arc<Tree>` ne recopie pas l'arbre, juste le compteur |
| *lifetime* (`'a`) | rien d'équivalent | une annotation qui dit « cette référence ne survit pas à ce dont elle vient ». Presque jamais utile d'en parler à l'oral |

## Exécution

| Rust | En Python | Où l'analogie casse |
|---|---|---|
| `panic!` | une exception non rattrapée | l'usage est plus strict : un panic signale un bug du programme, pas une donnée invalide (qui, elle, se traite en valeur) |
| *thread* | `threading.Thread` | il n'y a pas de GIL : deux fils Rust calculent réellement en parallèle |
| GIL | le verrou global de CPython | il existe toujours **dans la partie Python embarquée** de StokesForge ; c'est tout l'objet de l'invariant 9 |
| `unsafe` | `ctypes` | absent du code métier de StokesForge |

## Outils du dépôt

| Commande | Équivalent mental |
|---|---|
| `cargo build` | compiler |
| `cargo test` | `pytest` |
| `cargo fmt` | `black` |
| `cargo clippy` | `ruff` / `flake8`, mais bloquant en CI |

## Le mot qu'il vaut mieux ne pas dire

« *Borrow checker* », « monomorphisation », « *lifetime* », « *trait object* »,
« `Send`/`Sync` » : ces mots n'ajoutent rien à une explication destinée à
quelqu'un qui ne codera pas en Rust. Dire ce qu'ils **produisent** (« le
compilateur interdit que deux endroits modifient la même chose en même temps »)
plutôt que leur nom.
