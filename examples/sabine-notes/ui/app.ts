import { appWindow, type ControlsOverlay } from "@lantharos/sabine";
import { commands } from "./bindings";

interface Note {
  id: string;
  title: string;
  body: string;
  saved: boolean;
}

const defaultNotes: Note[] = [
  {
    id: "product-notes",
    title: "Product notes",
    body: "Keep the app monochrome, calm, and functional.\nMake editing fast enough to feel native.",
    saved: true,
  },
  {
    id: "runtime-checklist",
    title: "Runtime checklist",
    body: "Install the embedded CEF runtime locally.\nShow a native installing window while it downloads.",
    saved: true,
  },
  {
    id: "design-pass",
    title: "Design pass",
    body: "Use square sidebars and restrained controls.\nKeep the editor focused on the note.",
    saved: true,
  },
];

const storeKey = "sabine.notes.example.v1";

const list = document.querySelector<HTMLElement>("#note-list")!;
const count = document.querySelector<HTMLElement>("#note-count")!;
const status = document.querySelector<HTMLElement>("#status")!;
const title = document.querySelector<HTMLInputElement>("#note-title")!;
const body = document.querySelector<HTMLTextAreaElement>("#note-body")!;
const newButton = document.querySelector<HTMLButtonElement>("#new-note")!;
const saveButton = document.querySelector<HTMLButtonElement>("#save-note")!;
const deleteButton = document.querySelector<HTMLButtonElement>("#delete-note")!;

const stored = loadStore();
let notes = stored.notes;
let selected = Math.min(stored.selected, notes.length - 1);

function loadStore(): { notes: Note[]; selected: number } {
  const value = JSON.parse(localStorage.getItem(storeKey) ?? "null");
  const saved: unknown[] = Array.isArray(value?.notes) ? value.notes : [];
  return {
    notes: saved.length
      ? saved.map((note: any) => ({
          id: String(note.id),
          title: String(note.title || "Untitled"),
          body: String(note.body ?? ""),
          saved: true,
        }))
      : defaultNotes.map((note) => ({ ...note })),
    selected: Number.isInteger(value?.selected) ? value.selected : 0,
  };
}

function persist() {
  localStorage.setItem(
    storeKey,
    JSON.stringify({
      selected,
      notes: notes.map(({ id, title, body }) => ({ id, title, body })),
    }),
  );
}

function currentNote() {
  return notes[selected];
}

function showError(error: unknown) {
  status.textContent = error instanceof Error ? error.message : String(error);
}

function renderList() {
  list.replaceChildren(
    ...notes.map((note, index) => {
      const button = document.createElement("button");
      button.textContent = note.saved ? note.title || "Untitled" : `${note.title || "Untitled"} *`;
      button.className = index === selected ? "active" : "";
      button.addEventListener("click", () => {
        persistCurrentFields();
        selected = index;
        persist();
        render();
      });
      return button;
    }),
  );
  count.textContent = `${notes.length} ${notes.length === 1 ? "note" : "notes"}`;
}

function renderEditor() {
  const note = currentNote();
  title.value = note.title;
  body.value = note.body;
  document.title = `${note.saved ? "" : "*"}Notes`;
}

function render() {
  renderList();
  renderEditor();
}

function persistCurrentFields() {
  const note = currentNote();
  note.title = title.value.trimStart();
  note.body = body.value;
}

function showControls(overlay: ControlsOverlay) {
  document.documentElement.dataset.controls = overlay.left || overlay.right ? "system" : "page";
}

title.addEventListener("input", () => {
  currentNote().title = title.value;
  currentNote().saved = false;
  renderList();
});

body.addEventListener("input", () => {
  currentNote().body = body.value;
  currentNote().saved = false;
});

newButton.addEventListener("click", async () => {
  persistCurrentFields();
  status.textContent = "";
  try {
    const created = await commands.notesCreate({ title: "Untitled" });
    notes.push({ id: created.id, title: created.title, body: "", saved: false });
  } catch (error) {
    showError(error);
    return;
  }
  selected = notes.length - 1;
  persist();
  render();
  title.focus();
  title.select();
});

saveButton.addEventListener("click", () => {
  persistCurrentFields();
  currentNote().saved = true;
  persist();
  renderList();
});

deleteButton.addEventListener("click", () => {
  if (notes.length === 1) {
    notes = [{ id: crypto.randomUUID(), title: "Untitled", body: "", saved: false }];
    selected = 0;
  } else {
    notes.splice(selected, 1);
    selected = Math.min(selected, notes.length - 1);
  }
  persist();
  render();
});

document.addEventListener("keydown", (event) => {
  if (!(event.ctrlKey || event.metaKey)) {
    return;
  }
  if (event.key.toLowerCase() === "s") {
    event.preventDefault();
    saveButton.click();
  }
  if (event.key.toLowerCase() === "n") {
    event.preventDefault();
    newButton.click();
  }
});

window.addEventListener("beforeunload", () => {
  persistCurrentFields();
  persist();
});

appWindow.onControlsOverlayChanged(showControls);
appWindow.revealWhen(appWindow.controlsOverlay().then(showControls, showError));
render();
