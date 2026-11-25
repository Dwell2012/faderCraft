#ifndef AppVersion
  #error AppVersion must be supplied with /DAppVersion=...
#endif
#ifndef SourceDir
  #error SourceDir must be supplied with /DSourceDir=...
#endif

[Setup]
AppId={{B9B04F2E-7D2B-4AD7-9DC0-9AC17B44A014}
AppName=faderCraft
AppVersion={#AppVersion}
AppPublisher=faderCraft
AppPublisherURL=https://github.com/Dwell2012/faderCraft
AppUpdatesURL=https://github.com/Dwell2012/faderCraft/releases
DefaultDirName={localappdata}\Programs\faderCraft
DefaultGroupName=faderCraft
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=no
OutputDir=output
OutputBaseFilename=faderCraft-{#AppVersion}-windows-x64-setup
SetupIconFile=..\..\crates\app\assets\fadercraft.ico
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\faderCraft.exe
WizardStyle=modern

[Files]
Source: "{#SourceDir}\djcraft.exe"; DestDir: "{app}"; DestName: "faderCraft.exe"; Flags: ignoreversion
Source: "{#SourceDir}\DirectML.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\faderCraft"; Filename: "{app}\faderCraft.exe"
Name: "{autodesktop}\faderCraft"; Filename: "{app}\faderCraft.exe"; Tasks: desktopicon

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked
