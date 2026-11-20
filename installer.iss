#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

[Setup]
AppName=Star Citizen Presence
AppVersion={#AppVersion}
AppPublisher=syntafin
AppPublisherURL=https://github.com/syntafin/starcitizen-rpc
DefaultDirName={localappdata}\Programs\StarCitizenPresence
DefaultGroupName=Star Citizen Presence
SetupIconFile=images\STARCITIZEN_WHITE.ico
UninstallDisplayIcon={app}\star-citizen-presence.exe
OutputDir=dist
OutputBaseFilename=StarCitizenPresence-Setup
Compression=lzma2
SolidCompression=yes
PrivilegesRequired=lowest

[Tasks]
Name: "autostart"; Description: "Mit Windows starten (Autostart)"; GroupDescription: "Optionen:"

[Files]
Source: "target\release\star-citizen-presence.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "config.toml"; DestDir: "{app}"; Flags: onlyifdoesntexist

[Icons]
Name: "{group}\Star Citizen Presence"; Filename: "{app}\star-citizen-presence.exe"; WorkingDir: "{app}"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "StarCitizenPresence"; ValueData: """{app}\star-citizen-presence.exe"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\star-citizen-presence.exe"; Description: "Star Citizen Presence jetzt starten"; Flags: nowait postinstall skipifsilent
