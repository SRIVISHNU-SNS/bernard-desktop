export function isImageFile(file: File): boolean {
  if (file.type.startsWith("image/")) return true;
  return /\.(png|jpe?g|webp|gif|bmp)$/i.test(file.name);
}

export function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string" && reader.result.startsWith("data:image/")) {
        resolve(reader.result);
      } else {
        reject(new Error("That file is not an image Bernard can read."));
      }
    };
    reader.onerror = () => reject(new Error("Bernard could not read that file."));
    reader.readAsDataURL(file);
  });
}

export function joinModelsPath(dataDir: string, file: string): string {
  const base = dataDir.replace(/[\\/]+$/, "");
  return `${base}\\models\\${file}`;
}

export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}
