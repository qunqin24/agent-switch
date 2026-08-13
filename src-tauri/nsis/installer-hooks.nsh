; Branded installer copy for the four languages supported by Agent Switch.
LangString agentSwitchWelcomeTitle ${LANG_ENGLISH} "Install Agent Switch"
LangString agentSwitchWelcomeTitle ${LANG_SIMPCHINESE} "安装 Agent Switch"
LangString agentSwitchWelcomeTitle ${LANG_TRADCHINESE} "安裝 Agent Switch"
LangString agentSwitchWelcomeTitle ${LANG_JAPANESE} "Agent Switch をインストール"
LangString agentSwitchWelcomeText ${LANG_ENGLISH} "Version ${VERSION} will be installed for the current user.$\r$\n$\r$\nA new installation does not require administrator permission. Click Install to continue."
LangString agentSwitchWelcomeText ${LANG_SIMPCHINESE} "即将为当前用户安装 ${VERSION} 版本。$\r$\n$\r$\n全新安装无需管理员权限，点击安装即可继续。"
LangString agentSwitchWelcomeText ${LANG_TRADCHINESE} "即將為目前使用者安裝 ${VERSION} 版本。$\r$\n$\r$\n全新安裝無需系統管理員權限，點擊安裝即可繼續。"
LangString agentSwitchWelcomeText ${LANG_JAPANESE} "現在のユーザー向けにバージョン ${VERSION} をインストールします。$\r$\n$\r$\n新規インストールに管理者権限は不要です。インストールをクリックして続行してください。"
LangString agentSwitchInstall ${LANG_ENGLISH} "Install"
LangString agentSwitchInstall ${LANG_SIMPCHINESE} "安装"
LangString agentSwitchInstall ${LANG_TRADCHINESE} "安裝"
LangString agentSwitchInstall ${LANG_JAPANESE} "インストール"
LangString agentSwitchFinishTitle ${LANG_ENGLISH} "Agent Switch is ready"
LangString agentSwitchFinishTitle ${LANG_SIMPCHINESE} "Agent Switch 已安装完成"
LangString agentSwitchFinishTitle ${LANG_TRADCHINESE} "Agent Switch 已安裝完成"
LangString agentSwitchFinishTitle ${LANG_JAPANESE} "Agent Switch の準備ができました"
LangString agentSwitchFinishText ${LANG_ENGLISH} "Agent Switch was installed successfully."
LangString agentSwitchFinishText ${LANG_SIMPCHINESE} "Agent Switch 已成功安装。"
LangString agentSwitchFinishText ${LANG_TRADCHINESE} "Agent Switch 已成功安裝。"
LangString agentSwitchFinishText ${LANG_JAPANESE} "Agent Switch が正常にインストールされました。"

Function WelcomePageShow
  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 ${WM_SETTEXT} 0 "STR:$(agentSwitchInstall)"
FunctionEnd
