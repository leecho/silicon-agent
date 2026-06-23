// 主题应用：默认浅色，支持深/浅切换（见 ADR-0003）。
// 偏好主题项取值 system/light/dark。深色通过在 documentElement 上挂 .theme-dark 覆盖
// token 实现，挂在根节点是为了让弹层 portal（如 Select 下拉）也继承同一主题。

export type ThemePreference = "system" | "light" | "dark" | string;

/** 解析偏好主题为是否深色：light→否，dark→是，system→跟随 OS。默认浅色。 */
export function resolveDark(theme: ThemePreference): boolean {
  if (theme === "dark") return true;
  if (theme === "light") return false;
  // system：跟随操作系统；无法判断时回退浅色。
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-color-scheme: dark)").matches;
}

/** 把主题应用到 documentElement。 */
export function applyTheme(theme: ThemePreference): void {
  if (typeof document === "undefined") return;
  document.documentElement.classList.toggle("theme-dark", resolveDark(theme));
}
