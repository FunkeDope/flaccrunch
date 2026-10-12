import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { MenuBar, type MenuDef } from "./MenuBar";

function setup(overrides: Partial<{ onOpen: () => void; onSave: () => void; onTheme: () => void }> = {}) {
  const onOpen = overrides.onOpen ?? vi.fn();
  const onSave = overrides.onSave ?? vi.fn();
  const onTheme = overrides.onTheme ?? vi.fn();
  const menus: MenuDef[] = [
    {
      id: "file",
      label: "&File",
      items: [
        { label: "&Open…", shortcut: "Ctrl+O", onSelect: onOpen },
        { type: "separator" },
        { label: "&Save", shortcut: "Ctrl+S", onSelect: onSave, disabled: true },
        { label: "E&xit", shortcut: "Alt+F4", nativeShortcut: true, onSelect: vi.fn() },
      ],
    },
    {
      id: "view",
      label: "&View",
      items: [{ type: "submenu", label: "&Theme", items: [{ label: "Dark", radio: true, checked: true, onSelect: onTheme }] }],
    },
    { id: "dev", label: "&Dev", hidden: true, items: [{ label: "Reload", onSelect: vi.fn() }] },
  ];
  render(<MenuBar menus={menus} />);
  return { onOpen, onSave, onTheme };
}

describe("MenuBar", () => {
  it("renders visible menus only", () => {
    setup();
    expect(screen.getAllByRole("menuitem").map((b) => b.textContent)).toEqual(["File", "View"]);
  });

  it("opens on press and runs an item on click", () => {
    const { onOpen } = setup();
    fireEvent.pointerDown(screen.getAllByRole("menuitem")[0], { button: 0 });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.click(screen.getByText("pen…").closest("button")!);
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("runs shortcuts as accelerators, skipping disabled and OS-handled ones", () => {
    const { onOpen, onSave } = setup();
    fireEvent.keyDown(window, { key: "o", ctrlKey: true });
    fireEvent.keyDown(window, { key: "s", ctrlKey: true });
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("opens a menu with Alt+mnemonic and picks with the keyboard", () => {
    const { onTheme } = setup();
    fireEvent.keyDown(window, { key: "v", altKey: true });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "ArrowRight" }); // into the Theme submenu
    fireEvent.keyDown(window, { key: "Enter" });
    expect(onTheme).toHaveBeenCalledTimes(1);
  });

  it("closes on Escape", () => {
    setup();
    fireEvent.keyDown(window, { key: "f", altKey: true });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
