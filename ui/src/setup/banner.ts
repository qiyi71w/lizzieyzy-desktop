// LizzieFrame.layoutEngineStartupStatus: bottom-left, just above BottomToolbar.
import { registerViewHandler } from "../app";
import { t } from "../i18n";
import { openAutoSetup, resourceView } from "./dialog";
import { action } from "./setupText";

export function mountSetupBanner(): void {
  const button = action("engine.startup.repair", t("EngineStartup.needsRepair"), () => openAutoSetup("overview"));
  button.className = "setup-startup-banner"; button.hidden = true;
  document.body.append(button);
  registerViewHandler(() => { button.hidden = !resourceView()?.startupNeedsRepair; });
}
