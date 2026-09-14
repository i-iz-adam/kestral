import type { ComponentType } from "react";
import type { StepProps } from "./types";
import Welcome from "./steps/Welcome";
import OmniRouteConfig from "./steps/OmniRouteConfig";
import Defaults from "./steps/Defaults";
import Workspace from "./steps/Workspace";
import PythonSetup from "./steps/PythonSetup";
import Finish from "./steps/Finish";
import Stop from "./steps/Stop";

// Add new steps here AND to step_registry() in src-tauri/src/setup.rs,
// using the same id string in both places. Existing users who already
// completed setup will only be shown ids that aren't in their saved
// completed_steps list, so appending here is safe for people upgrading.
export const stepComponents: Record<string, ComponentType<StepProps>> = {
  welcome: Welcome,
  omniroute: OmniRouteConfig,
  defaults: Defaults,
  workspace: Workspace,
  python: PythonSetup,
  stop: Stop,
  finish: Finish,
};
