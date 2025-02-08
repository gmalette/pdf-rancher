/** Mermaid Diagram
 * stateDiagram
 *     [*] --> List
 *
 *     List --> Importing: "rancher//will-open-files"
 *     Importing --> List: "rancher//did-open-files"
 *     List --> DraggingOver: "tauri//drag-over"
 *     DraggingOver --> List: "tauri//drag-leave"
 *     DraggingOver --> List: "tauri//drag-drop"
 *     List --> Exporting: "rancher//will-export"
 *     Exporting --> List: "rancher//did-export"
 */
export const LIST = Symbol("LIST");
export const DRAGGING_OVER = Symbol("DRAGGING_OVER");
export const FOCUSED = Symbol("FOCUSED");
export const IMPORTING = Symbol("IMPORTING");
export const EXPORTING = Symbol("EXPORTING");
export const LICENSE = Symbol("LICENSE");

export type List = { type: typeof LIST };
export type DraggingOver = { type: typeof DRAGGING_OVER };
export type Focused = { type: typeof FOCUSED, ordering: number };
export type Importing = { type: typeof IMPORTING };
export type Exporting = { type: typeof EXPORTING };
export type License = { type: typeof LICENSE };

export type UiState =
  List |
  DraggingOver |
  Focused |
  Importing |
  Exporting |
  License;

export function ListState(): UiState {
  return { type: LIST };
}

export function DraggingOverState(): UiState {
  return { type: DRAGGING_OVER };
}

export function FocusedState(ordering: number): UiState {
  return { type: FOCUSED, ordering };
}

export function ImportingState(): UiState {
  return { type: IMPORTING };
}

export function ExportingState(): UiState {
  return { type: EXPORTING };
}

export function LicenseState(): UiState {
  return { type: LICENSE };
}
