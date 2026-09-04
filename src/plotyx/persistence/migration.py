"""Vérification de version de schéma et migration.

**Non négociable** (roadmap phase 4) : le numéro de schéma est le premier
attribut posé sur tout fichier `.plx`, dès la version 0.0.1. C'est ce qui
permet de distinguer, à l'ouverture, trois cas :

* fichier à la version courante → lecture directe ;
* fichier à une version **antérieure** → `migrate()` le met à niveau avant
  lecture ;
* fichier à une version **future** (écrit par une version de PlotYx plus
  récente que celle qui l'ouvre) → refus propre, message clair, jamais un
  plantage — `SchemaVersionError`.

## Les versions existantes

| Schéma | Introduit par | Ce qui change |
|---|---|---|
| 1 | phase 4 | arbre projet, colonnes, métadonnées |
| 2 | phase 6 | attribut `plot_spec_json` sur les groupes `graph` |

Le passage de 1 à 2 est **additif** : un fichier de schéma 1 contient des
graphes sans contenu, ce qui est exactement l'état d'un graphe neuf en
schéma 2 (`Graph.plot_spec is None`). La migration n'a donc rien à
réécrire, et surtout : elle ne doit **pas** inventer un spec vide, ce qui
transformerait un « graphe jamais tracé » en « graphe explicitement vide »
et ferait diverger un round-trip. Elle est ici pour prendre la décision
noir sur blanc, pas parce qu'il y aurait des octets à déplacer.

## Comment ajoutera-t-on la version 3

1. Incrémenter `schema.CURRENT_SCHEMA_VERSION`.
2. Ajouter dans `migrate()` un bloc `if from_version < 3: ...` qui
   transforme le fichier ouvert en écriture vers la forme attendue par le
   lecteur courant.
3. Ne jamais retirer les blocs précédents : un fichier de 2026 doit encore
   s'ouvrir en 2030.
"""

from __future__ import annotations

import h5py

from plotyx.persistence.errors import CorruptedProjectFileError, SchemaVersionError
from plotyx.persistence.schema import ATTR_SCHEMA_VERSION, CURRENT_SCHEMA_VERSION

__all__ = ["check_version", "migrate"]


def check_version(file: h5py.File) -> int:
    """Lit et valide `plotyx_schema_version`. Lève, ne plante jamais.

    Retourne la version lue si elle est utilisable (égale ou antérieure à
    `CURRENT_SCHEMA_VERSION` — `migrate()` reste à appeler pour le cas
    antérieur).
    """
    raw = file.attrs.get(ATTR_SCHEMA_VERSION)
    if raw is None:
        raise CorruptedProjectFileError(
            f"{file.filename!r} n'a pas d'attribut de version de schéma : "
            "ce n'est pas un fichier projet PlotYx, ou il est corrompu."
        )
    version = int(raw)
    if version > CURRENT_SCHEMA_VERSION:
        raise SchemaVersionError(
            f"{file.filename!r} a été enregistré avec une version de PlotYx "
            f"plus récente (schéma {version}) que celle-ci, qui ne sait lire "
            f"que jusqu'au schéma {CURRENT_SCHEMA_VERSION}. Mettez PlotYx à "
            "jour pour ouvrir ce projet."
        )
    return version


def migrate(file: h5py.File, from_version: int) -> None:
    """Met `file` à niveau du schéma `from_version` vers le schéma courant.

    Ne fait rien tant qu'une seule version de schéma existe — voir le
    docstring du module pour la marche à suivre le jour où `from_version`
    peut valoir autre chose que `CURRENT_SCHEMA_VERSION`.
    """
    if from_version == CURRENT_SCHEMA_VERSION:
        return
    if from_version == 1:
        # 1 → 2 : ajout de `plot_spec_json` sur les graphes. Purement
        # additif, et volontairement sans écriture (voir le docstring du
        # module) : l'absence de l'attribut est déjà la bonne
        # représentation d'un graphe sans contenu. Un fichier de schéma 1
        # s'ouvre donc en lecture seule, sans être réécrit.
        return
    raise CorruptedProjectFileError(
        f"aucune migration connue depuis le schéma {from_version}"
    )
