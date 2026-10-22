; Installeur Windows de StokesForge (#332), compilé par scripts/app-dist-windows.sh
; avec Inno Setup 6 (ISCC.exe) :
;   ISCC /DAppVersion=1.0.0 /DSourceDir=<dossier assemblé> /DOutputDir=<sortie> stokesforge.iss
; SourceDir est le dossier de staging déjà éprouvé par `stokesforge-selftest` ;
; l'installeur n'ajoute rien à ce qu'il contient. Pas de signature de code pour
; l'instant : Windows affiche « éditeur inconnu » (SmartScreen) au lancement.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #error SourceDir doit désigner le dossier assemblé par scripts/app-dist-windows.sh
#endif
#ifndef Suffix
  #define Suffix ""
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
; Identité stable de l'application : ne jamais la changer, elle relie mises à
; jour et désinstallation.
AppId={{B7D2C4F0-5C1E-4B7A-9E36-2A8F1D4C6E91}
AppName=StokesForge
AppVersion={#AppVersion}
AppPublisher=StokesForge
DefaultDirName={autopf}\StokesForge
DefaultGroupName=StokesForge
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename=StokesForge-{#AppVersion}{#Suffix}-windows-setup
LicenseFile={#SourceDir}\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Par utilisateur par défaut (aucun droit d'administrateur) ; l'assistant
; propose l'installation pour tous les utilisateurs.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
UninstallDisplayIcon={app}\stokesforge-app.exe
ChangesAssociations=yes

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{group}\StokesForge"; Filename: "{app}\stokesforge-app.exe"
Name: "{group}\Uninstall StokesForge"; Filename: "{uninstallexe}"

[Registry]
; Association des projets .stf : « Ouvrir avec » StokesForge, sans écraser le
; programme par défaut déjà choisi par l'utilisateur (OpenWithProgids).
Root: HKA; Subkey: "Software\Classes\.stf\OpenWithProgids"; ValueType: string; ValueName: "StokesForge.Project"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\StokesForge.Project"; ValueType: string; ValueName: ""; ValueData: "StokesForge project"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\StokesForge.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\stokesforge-app.exe"" ""%1"""

[UninstallDelete]
; CPython écrit des __pycache__ à l'exécution : ils ne sont pas des fichiers
; installés, sans cela le dossier survivrait à la désinstallation.
Type: filesandordirs; Name: "{app}\lib"

[Run]
Filename: "{app}\stokesforge-app.exe"; Description: "Launch StokesForge"; Flags: nowait postinstall skipifsilent
