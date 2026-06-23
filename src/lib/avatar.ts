/** 头像展示：仅当 `avatar` 是 emoji（短文本，非路径/文件名/URL）时按文本渲染；
 * 图片路径（如导入团队包里的 `avatars/x.png`）一律回退到默认图标——本应用不加载这类相对图片。
 * 返回可直接渲染的 emoji 文本，或 null（调用方据此回退到图标）。 */
export function avatarEmoji(avatar?: string | null): string | null {
  const v = avatar?.trim();
  if (!v) return null;
  if (v.includes("/") || v.includes("\\")) return null; // 路径
  if (/\.(png|jpe?g|gif|webp|svg|ico)$/i.test(v)) return null; // 图片文件名
  if (/^https?:/i.test(v)) return null; // URL
  return v;
}
