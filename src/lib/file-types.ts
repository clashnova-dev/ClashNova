export type FileKind =
  "pdf" | "document" | "sheet" | "slides" | "archive" | "image" | "audio" | "video" | "code" | "text" | "generic";
const groups: [FileKind, string[]][] = [
  ["pdf", ["pdf"]],
  ["document", ["doc", "docx", "odt", "rtf", "pages"]],
  ["sheet", ["xls", "xlsx", "csv", "ods", "numbers"]],
  ["slides", ["ppt", "pptx", "odp", "key"]],
  ["archive", ["zip", "rar", "7z", "gz", "tar", "bz2", "xz"]],
  ["image", ["png", "jpg", "jpeg", "gif", "webp", "svg", "heic", "avif", "bmp", "tiff"]],
  ["audio", ["mp3", "wav", "flac", "aac", "m4a", "ogg"]],
  ["video", ["mp4", "mov", "mkv", "webm", "avi", "m4v"]],
  ["code", ["js", "ts", "tsx", "jsx", "py", "go", "rs", "java", "html", "css", "json", "yaml", "yml", "sh", "sql"]],
  ["text", ["txt", "md", "log", "ini", "conf"]],
];
export function fileType(name: string): { kind: FileKind; label: string } {
  const base = name.split(/[\\/]/).pop() || "";
  const dot = base.lastIndexOf("."),
    extension = dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
  const kind = groups.find(([, extensions]) => extensions.includes(extension))?.[0] || "generic";
  return { kind, label: kind === "generic" ? "FILE" : extension.toUpperCase().slice(0, 4) };
}
