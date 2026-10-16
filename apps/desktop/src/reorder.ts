// Drag-to-reorder for `.lrow[data-name]` rows inside a container.

export function enableReorder(container: HTMLElement, onMove: (name: string, index: number) => void): void {
  let dragged: HTMLElement | null = null;
  const rows = () => Array.from(container.querySelectorAll<HTMLElement>(".lrow[draggable=true]"));
  const clear = () => rows().forEach((r) => r.classList.remove("drop-above", "drop-below"));

  container.addEventListener("dragstart", (e) => {
    const row = (e.target as HTMLElement).closest<HTMLElement>(".lrow");
    if (!row) return;
    dragged = row;
    row.classList.add("dragging");
    e.dataTransfer?.setData("text/plain", row.dataset.name ?? "");
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  });
  container.addEventListener("dragover", (e) => {
    const row = (e.target as HTMLElement).closest<HTMLElement>(".lrow");
    if (!row || !dragged || row === dragged) return;
    e.preventDefault();
    clear();
    const r = row.getBoundingClientRect();
    row.classList.add(e.clientY < r.top + r.height / 2 ? "drop-above" : "drop-below");
  });
  container.addEventListener("dragleave", (e) => {
    if (!container.contains(e.relatedTarget as Node)) clear();
  });
  container.addEventListener("drop", (e) => {
    e.preventDefault();
    const row = (e.target as HTMLElement).closest<HTMLElement>(".lrow");
    if (!row || !dragged || row === dragged) return;
    const all = rows();
    const from = all.indexOf(dragged);
    let to = all.indexOf(row) + (row.classList.contains("drop-below") ? 1 : 0);
    if (from < to) to -= 1;
    clear();
    if (to !== from) onMove(dragged.dataset.name ?? "", Number(row.dataset.base ?? 0) + to);
  });
  container.addEventListener("dragend", () => {
    dragged?.classList.remove("dragging");
    dragged = null;
    clear();
  });
}
