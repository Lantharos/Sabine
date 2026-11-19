import { menu, spellcheck, type MenuContext, type MenuRole } from "@lantharos/sabine";

interface Entry {
  label: string;
  run: () => unknown;
}

const panel = document.createElement("div");
panel.className = "context-menu";
panel.hidden = true;
document.body.append(panel);

function edit(label: string, role: MenuRole, applies: boolean): Entry[] {
  return applies ? [{ label, run: () => menu.run(role) }] : [];
}

function groups(context: MenuContext): Entry[][] {
  const misspelled = context.misspelled;
  return [
    context.suggestions.map((word) => ({ label: word, run: () => spellcheck.replace(word) })),
    misspelled ? [{ label: "Add to dictionary", run: () => spellcheck.addWord(misspelled) }] : [],
    [...edit("Undo", "undo", context.canUndo), ...edit("Redo", "redo", context.canRedo)],
    [
      ...edit("Cut", "cut", context.canCut),
      ...edit("Copy", "copy", context.canCopy),
      ...edit("Paste", "paste", context.canPaste),
    ],
    edit("Select all", "selectAll", context.editable && context.canSelectAll),
  ].filter((group) => group.length > 0);
}

function entryButton(entry: Entry) {
  const button = document.createElement("button");
  button.textContent = entry.label;
  button.addEventListener("mousedown", (event) => event.preventDefault());
  button.addEventListener("click", () => {
    close();
    entry.run();
  });
  return button;
}

function close() {
  panel.hidden = true;
}

menu.onContext((context) => {
  const shown = groups(context);
  if (shown.length === 0) {
    close();
    return;
  }
  panel.replaceChildren(
    ...shown.flatMap((group, index) => [
      ...(index > 0 ? [document.createElement("hr")] : []),
      ...group.map(entryButton),
    ]),
  );
  panel.hidden = false;
  const { width, height } = panel.getBoundingClientRect();
  panel.style.left = `${Math.min(context.x, innerWidth - width - 6)}px`;
  panel.style.top = `${Math.min(context.y, innerHeight - height - 6)}px`;
});

addEventListener("pointerdown", (event) => {
  if (!panel.contains(event.target as Node)) {
    close();
  }
});
addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    close();
  }
});
addEventListener("blur", close);
