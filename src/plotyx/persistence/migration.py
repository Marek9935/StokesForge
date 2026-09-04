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

Il n'existe pour l'instant qu'une version de schéma (1), donc `migrate()`
est un stub qui ne peut pas être atteint autrement qu'en test. Il est écrit
maintenant, pas laissé "à faire plus tard", parce que c'est précisément le
risque identifié par la roadmap : un format non versionné rend les projets
utilisateurs illisibles après une mise à jour.

## Comment ajoutera-t-on la version 2

1. Incrémenter `schema.CURRENT_SCHEMA_VERSION`.
2. Ajouter dans `migrate()` un bloc `if from_version < 2: ...` qui
   transforme le fichier ouvert en écriture (`h5py.File(path, "r+")`) vers
   la forme attendue par le lecteur courant.
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
    # pragma: no cover - inatteignable tant que CURRENT_SCHEMA_VERSION == 1
    raise CorruptedProjectFileError(
        f"aucune migration connue depuis le schéma {from_version}"
    )  # pragma: no cover
