
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\dist\stage"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
AppId={{6C1F2B7A-3E84-4F55-9D2C-8A4E0B5D7F31}
AppName=MarkdownShot
AppVersion={#AppVersion}
AppVerName=MarkdownShot {#AppVersion}
AppPublisher=MarkdownShot
AppPublisherURL=https://github.com/MrSacre/MarkdownShot
AppSupportURL=https://github.com/MrSacre/MarkdownShot/issues
AppUpdatesURL=https://github.com/MrSacre/MarkdownShot/releases
DefaultDirName={autopf}\MarkdownShot
DefaultGroupName=MarkdownShot
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=MarkdownShot-{#AppVersion}-windows-x64-setup
SetupIconFile=..\assets\markdownshot.ico
UninstallDisplayIcon={app}\markdownshot.exe
UninstallDisplayName=MarkdownShot
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=no
RestartApplications=no

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"

[CustomMessages]
en.Autostart=Start MarkdownShot with Windows
fr.Autostart=Démarrer MarkdownShot avec Windows
en.Launch=Launch MarkdownShot now
fr.Launch=Lancer MarkdownShot maintenant
en.Options=Options:
fr.Options=Options :
en.RemoveSettings=Also remove your MarkdownShot settings (%APPDATA%\MarkdownShot)?%n%nYour notes and their images are not deleted.
fr.RemoveSettings=Supprimer aussi vos réglages MarkdownShot (%APPDATA%\MarkdownShot) ?%n%nVos notes et leurs images ne sont pas supprimées.

[Tasks]
Name: "autostart"; Description: "{cm:Autostart}"; GroupDescription: "{cm:Options}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[InstallDelete]
Type: files; Name: "{app}\LISEZ-MOI.txt"
Type: filesandordirs; Name: "{app}\licences"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\MarkdownShot"; Filename: "{app}\markdownshot.exe"
Name: "{autodesktop}\MarkdownShot"; Filename: "{app}\markdownshot.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "MarkdownShot"; ValueData: """{app}\markdownshot.exe"""; Tasks: autostart

[Run]
Filename: "{app}\markdownshot.exe"; Description: "{cm:Launch}"; Flags: nowait postinstall skipifsilent
Filename: "{app}\markdownshot.exe"; Flags: nowait runasoriginaluser; Check: Relaunch

[Code]
procedure CloseRunningApp();
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM markdownshot.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Sleep(500);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  CloseRunningApp();
  Result := '';
end;

function Relaunch(): Boolean;
begin
  Result := WizardSilent() and (Pos('/RELAUNCH', UpperCase(GetCmdTail())) > 0);
end;

function InitializeUninstall(): Boolean;
begin
  CloseRunningApp();
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Settings: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'MarkdownShot');
    Settings := ExpandConstant('{userappdata}\MarkdownShot');
    if DirExists(Settings) and not UninstallSilent() then
      if MsgBox(CustomMessage('RemoveSettings'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(Settings, True, True, True);
  end;
end;
