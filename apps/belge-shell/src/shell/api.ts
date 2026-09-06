import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, FeatureState, MigrationReport, Settings } from "./types";

export const appInfo = () => invoke<AppInfo>("app_info");
export const enabledFeatures = () => invoke<FeatureState[]>("enabled_features");
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (next: Settings) => invoke<Settings>("save_settings", { next });
export const migrateLegacySettings = () => invoke<MigrationReport>("migrate_legacy_settings");
