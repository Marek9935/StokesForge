; Windows installer of StokesForge (#332), built by scripts/app-dist-windows.sh
; with Inno Setup 6 (ISCC.exe):
;   ISCC /DAppVersion=1.0.0 /DSourceDir=<assembled folder> /DOutputDir=<output> stokesforge.iss
; SourceDir is the staging folder already tested by `stokesforge-selftest`;
; the installer adds nothing to what it contains. No code signing for now:
; Windows shows "unknown publisher" (SmartScreen) at launch.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #error SourceDir must point to the folder assembled by scripts/app-dist-windows.sh
#endif
#ifndef Suffix
  #define Suffix ""
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
; Stable identity of the application: never change it, it links updates and
; uninstallation.
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
; Per user by default (no administrator rights); the wizard offers installation
; for all users.
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
; Association of .stf projects: "Open with" StokesForge, without overwriting the
; default program already chosen by the user (OpenWithProgids).
Root: HKA; Subkey: "Software\Classes\.stf\OpenWithProgids"; ValueType: string; ValueName: "StokesForge.Project"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\StokesForge.Project"; ValueType: string; ValueName: ""; ValueData: "StokesForge project"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\StokesForge.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\stokesforge-app.exe"" ""%1"""

[UninstallDelete]
; CPython writes __pycache__ at run time: they are not installed files, without
; this the folder would survive uninstallation.
Type: filesandordirs; Name: "{app}\lib"

[Run]
Filename: "{app}\stokesforge-app.exe"; Description: "Launch StokesForge"; Flags: nowait postinstall skipifsilent
