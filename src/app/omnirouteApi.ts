import { invoke } from "@tauri-apps/api/core";
import type { ModelsCache, ModelTestResult, OmniRouteConfigPayload } from "../types";

/**
 * Exposed OmniRoute API methods for managing models, configuration, and image generation settings.
 */

export async function fetchOmniRouteModels(): Promise<ModelsCache> {
  return invoke<ModelsCache>("fetch_omniroute_models");
}

export async function listImageModels(): Promise<string[]> {
  return invoke<string[]>("list_image_models");
}

export async function setDefaultModel(model: string | null): Promise<OmniRouteConfigPayload> {
  return invoke<OmniRouteConfigPayload>("set_default_model", { model });
}

export async function setDefaultImageModel(model: string | null): Promise<OmniRouteConfigPayload> {
  return invoke<OmniRouteConfigPayload>("set_default_image_model", { model });
}

export async function getOmniRouteConfig(): Promise<OmniRouteConfigPayload | null> {
  return invoke<OmniRouteConfigPayload | null>("get_omniroute_config");
}

export async function saveOmniRouteConfig(config: OmniRouteConfigPayload): Promise<void> {
  return invoke("save_omniroute_config", { config });
}

export async function testOmniRouteConnection(config: OmniRouteConfigPayload): Promise<boolean> {
  return invoke<boolean>("test_omniroute_connection", { config });
}

export async function testModel(model: string): Promise<ModelTestResult> {
  return invoke<ModelTestResult>("test_model", { model });
}

// Expose on global window object for accessibility via devtools or external scripts
if (typeof window !== "undefined") {
  (window as any).omniroute = {
    fetchModels: fetchOmniRouteModels,
    listImageModels,
    setDefaultModel,
    setDefaultImageModel,
    getConfig: getOmniRouteConfig,
    saveConfig: saveOmniRouteConfig,
    testConnection: testOmniRouteConnection,
    testModel,
  };
}
