export type Page = {
  preview_jpg: string
  dimensions: [number, number]
}

export type SourceFile = {
  pages: Page[]
  path: string,
}

export type Ordering = {
  id: number,
  source_file_index: number,
  page_index: number,
  enabled: boolean,
  rotation: number,
}

export type Project = {
  source_files: SourceFile[],
  ordering: Ordering[],
}

export function previewToDataUrl(preview_jpg: string) {
  return "data:image/jpg;base64," + preview_jpg
}
