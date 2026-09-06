import { invoke } from "@tauri-apps/api/core";
import type { AnalysisResult, Fix, WriteResult } from "./types";

export const analyzeDocument = (path: string) =>
  invoke<AnalysisResult>("ikincigoz_analyze_document", { path });
export const applyFixes = (path: string, fixes: Fix[], outputDir?: string) =>
  invoke<WriteResult>("ikincigoz_apply_fixes", { path, fixes, outputDir: outputDir ?? null });
export const previewFixes = (path: string, fixes: Fix[]) =>
  invoke<string[]>("ikincigoz_preview_fixes", { path, fixes });
