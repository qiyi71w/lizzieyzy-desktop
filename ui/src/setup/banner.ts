// The engine start-up notice (LizzieFrame.layoutEngineStartupStatus): a row between the toolbar
// and the workbench, so it covers no panel; the whole row opens the setup window.
import { registerViewHandler } from "../app";
import { t } from "../i18n";
import { openAutoSetup, resourceView } from "./dialog";
import { action } from "./setupText";

export function mountSetupBanner(): void {
  const button = action("engine.startup.repair", t("EngineStartup.needsRepair"), () => openAutoSetup("overview"));
  button.className = "setup-startup-banner"; button.hidden = true;
  document.getElementById("main")!.before(button);
  registerViewHandler(() => { button.hidden = !resourceView()?.startupNeedsRepair; });
}
