/**
 * 界面主题注册表。
 *
 * 一个主题 = 一套配色 + 一种画风, 分别打在 <html> 上:
 * - data-theme = 主题 id, 配色 token 按它切换 (classic.css; 手绘的配色就是 styles.css 本身);
 * - data-art   = 画风, 外形规则 (圆角 / 描边 / 字体) 与画风相关的组件按它切换
 *   (plain.css; 侧栏图标、Logo、实时路由图等组件读 useTheme().art)。
 * 以后加主题: 同一画风下换一套配色只需新增一个 id + 一段配色 CSS;
 * 新画风才需要动组件。
 *
 * index.html 里有一段首帧前的内联脚本同步这两个属性, 增删主题时要一起改。
 */
export type ThemeId = "classic" | "sketch";
export type ThemeArt = "plain" | "sketch";

export interface ThemeDef {
  id: ThemeId;
  art: ThemeArt;
  /** 主题名的 i18n key */
  labelKey: string;
}

/** 顺序即侧栏主题按钮的轮换顺序、设置页里按钮的排列顺序 */
export const THEMES: readonly ThemeDef[] = [
  { id: "classic", art: "plain", labelKey: "theme.name.classic" },
  { id: "sketch", art: "sketch", labelKey: "theme.name.sketch" },
];

export const DEFAULT_THEME_ID: ThemeId = "classic";

/** localStorage key; 与 index.html 内联脚本保持一致 */
export const THEME_ID_KEY = "cc-router-ui-theme";

export function isThemeId(v: unknown): v is ThemeId {
  return THEMES.some((t) => t.id === v);
}

export function themeDef(id: ThemeId): ThemeDef {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

/** 轮换到下一个主题 (侧栏主题按钮) */
export function nextThemeId(id: ThemeId): ThemeId {
  const i = THEMES.findIndex((t) => t.id === id);
  return THEMES[(i + 1) % THEMES.length].id;
}

export function applyThemeAttrs(id: ThemeId) {
  const root = document.documentElement;
  root.dataset.theme = id;
  root.dataset.art = themeDef(id).art;
}
