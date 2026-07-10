import type { AdapterManifest } from "./types.js";
/**
 * 动态发现所有可用的适配器。
 * 优先通过动态 import 加载各适配器包的 manifest.json；
 * 若加载失败则回退到内置 fallback 清单。
 * 新增适配器时需同时：1) 在 ADAPTER_PACKAGES 追加包名
 * 2) 在 BUILTIN_FALLBACK 追加对应 fallback 条目。
 */
export declare function getAvailableAdapters(): Promise<AdapterManifest[]>;
//# sourceMappingURL=registry.d.ts.map