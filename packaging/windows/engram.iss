#define AppVersion GetEnv("ENGRAM_VERSION")

[Setup]
AppId={{03BA64A2-7D3D-4CE9-8A4C-88A0F0A2A47A}
AppName=Engram
AppVersion={#AppVersion}
AppPublisher=Liwidale
AppPublisherURL=https://github.com/liwidale/engram
DefaultDirName={autopf}\Engram
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
WizardStyle=modern
LicenseFile=..\..\LICENSE
SetupIconFile=..\..\branding\icons\engram.ico
UninstallDisplayIcon={app}\engram.exe
OutputDir=..\..\dist
OutputBaseFilename=engram-v{#AppVersion}-windows-x64-setup

[Files]
Source: "..\..\rust-core\target\release\engram.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\rust-core\target\release\engram-mcp.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"

[Icons]
Name: "{autoprograms}\Engram"; Filename: "{app}\engram.exe"

[Registry]
Root: HKA; Subkey: "Software\Classes\.engram"; ValueType: string; ValueName: ""; ValueData: "Engram.Project"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.engram"; ValueType: string; ValueName: "Content Type"; ValueData: "application/vnd.engram+json"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Engram.Project"; ValueType: string; ValueName: ""; ValueData: "Engram project memory"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Engram.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\engram.exe,0"
Root: HKA; Subkey: "Software\Classes\Engram.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\engram.exe"" ""%1"""

[Run]
Filename: "{app}\engram.exe"; Description: "Open Engram"; Flags: nowait postinstall skipifsilent
