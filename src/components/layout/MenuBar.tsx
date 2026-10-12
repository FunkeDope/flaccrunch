import { Fragment, useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

/**
 * An application menu bar drawn in the page (the native title bar and menu
 * are turned off so every theme can style its own). Behaves like a Windows
 * menu bar: menus open on press and follow the pointer once one is open, Alt
 * focuses the bar and shows the mnemonics, Alt+letter opens a menu, arrows
 * move, Enter picks, Escape backs out. Item shortcuts ("Ctrl+O") also work as
 * global keyboard accelerators. Narrow windows collapse the menus into one
 * overflow button.
 *
 * Labels mark their mnemonic with "&", as in Win32: "&File", "E&xit".
 */

export type MenuEntry =
  | {
      type?: "item";
      label: string;
      shortcut?: string;
      /** The shortcut is shown but handled by the OS / web view (Alt+F4, F12). */
      nativeShortcut?: boolean;
      onSelect: () => void;
      disabled?: boolean;
      /** Checkbox item when a boolean; radio item when `radio` is also set. */
      checked?: boolean;
      radio?: boolean;
      hidden?: boolean;
    }
  | { type: "separator"; hidden?: boolean }
  | { type: "submenu"; label: string; items: MenuEntry[]; disabled?: boolean; hidden?: boolean };

export interface MenuDef {
  id: string;
  label: string;
  items: MenuEntry[];
  hidden?: boolean;
}

type ItemEntry = Extract<MenuEntry, { onSelect: () => void }>;
type SubmenuEntry = Extract<MenuEntry, { type: "submenu" }>;

const COMPACT_QUERY = "(max-width: 600px)";

function plainLabel(label: string): string {
  return label.replace(/&(.)/g, "$1");
}

function mnemonic(label: string): string | null {
  const m = label.match(/&([^&])/);
  return m ? m[1].toLowerCase() : null;
}

function Label({ text, underline }: { text: string; underline: boolean }) {
  const i = text.search(/&[^&]/);
  if (i < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, i)}
      <span className={underline ? "menu-mnemonic is-shown" : "menu-mnemonic"}>{text[i + 1]}</span>
      {text.slice(i + 2)}
    </>
  );
}

function visible(items: MenuEntry[]): MenuEntry[] {
  const shown = items.filter((e) => !e.hidden);
  // Drop leading, trailing and doubled separators left by hidden items.
  return shown.filter(
    (e, i) =>
      e.type !== "separator" ||
      (i > 0 && i < shown.length - 1 && shown[i - 1].type !== "separator")
  );
}

function isSelectable(e: MenuEntry): boolean {
  return e.type !== "separator" && !e.disabled;
}

function stepIndex(items: MenuEntry[], from: number, dir: 1 | -1): number {
  if (items.length === 0) return -1;
  for (let n = 1; n <= items.length; n++) {
    const i = (from + dir * n + items.length * 2) % items.length;
    if (isSelectable(items[i])) return i;
  }
  return -1;
}

/** "Ctrl+Shift+O" → does this keydown match? Alt combos are left to the OS. */
function matchesShortcut(shortcut: string, e: KeyboardEvent): boolean {
  const parts = shortcut.split("+").map((p) => p.trim().toLowerCase());
  const key = parts.pop() ?? "";
  if (parts.includes("alt")) return false;
  const ctrl = parts.includes("ctrl");
  const shift = parts.includes("shift");
  if ((e.ctrlKey || e.metaKey) !== ctrl || e.shiftKey !== shift || e.altKey) return false;
  const pressed = e.key.toLowerCase();
  if (key === "enter") return pressed === "enter";
  if (key === "esc") return pressed === "escape";
  if (key === "space") return pressed === " ";
  return pressed === key;
}

function* allItems(menus: MenuDef[]): Generator<ItemEntry> {
  const walk = function* (items: MenuEntry[]): Generator<ItemEntry> {
    for (const e of visible(items)) {
      if (e.type === "submenu") {
        if (!e.disabled) yield* walk(e.items);
      } else if (e.type !== "separator") {
        yield e;
      }
    }
  };
  for (const m of menus) yield* walk(m.items);
}

function useMediaQuery(query: string): boolean {
  const get = () => typeof window !== "undefined" && !!window.matchMedia?.(query).matches;
  const [matches, setMatches] = useState(get);
  useEffect(() => {
    const mq = window.matchMedia?.(query);
    if (!mq) return;
    const update = () => setMatches(mq.matches);
    update();
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, [query]);
  return matches;
}

interface PanelProps {
  items: MenuEntry[];
  active: number;
  subOpen: number | null;
  subActive: number;
  underline: boolean;
  onHover: (index: number) => void;
  onSubHover: (index: number) => void;
  onPick: (entry: ItemEntry) => void;
  className?: string;
}

/** Flips a dropdown to the other side when it would leave the window. */
function useEdgeFlip<T extends HTMLElement>(): React.RefObject<T | null> {
  const ref = useRef<T>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    if (rect.right > window.innerWidth - 2) el.classList.add("is-flipped");
  });
  return ref;
}

function ItemRow({
  entry,
  activeRow,
  underline,
  onHover,
  onPick,
  children,
}: {
  entry: ItemEntry | SubmenuEntry;
  activeRow: boolean;
  underline: boolean;
  onHover: () => void;
  onPick?: () => void;
  children?: ReactNode;
}) {
  const sub = entry.type === "submenu";
  const item = sub ? null : (entry as ItemEntry);
  const checkable = item?.checked !== undefined;
  const role = sub ? "menuitem" : checkable ? (item?.radio ? "menuitemradio" : "menuitemcheckbox") : "menuitem";
  const classes = [
    "menu-item",
    activeRow ? "is-active" : "",
    item?.checked ? "is-checked" : "",
    item?.radio ? "is-radio" : "",
    sub ? "has-submenu" : "",
    entry.disabled ? "is-disabled" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div className="menu-row">
      <button
        type="button"
        role={role}
        aria-checked={checkable ? !!item?.checked : undefined}
        aria-haspopup={sub ? "menu" : undefined}
        aria-expanded={sub ? !!children : undefined}
        disabled={entry.disabled}
        tabIndex={-1}
        className={classes}
        onMouseEnter={onHover}
        onClick={(e) => {
          e.stopPropagation();
          onPick?.();
        }}
      >
        <span className="menu-check" aria-hidden="true">
          {item?.checked ? (item.radio ? "●" : "✓") : ""}
        </span>
        <span className="menu-label">
          <Label text={entry.label} underline={underline} />
        </span>
        <span className="menu-shortcut">{item?.shortcut ?? ""}</span>
        <span className="menu-arrow" aria-hidden="true">{sub ? "›" : ""}</span>
      </button>
      {children}
    </div>
  );
}

function SubPanel({ items, active, underline, onHover, onPick }: {
  items: MenuEntry[];
  active: number;
  underline: boolean;
  onHover: (i: number) => void;
  onPick: (entry: ItemEntry) => void;
}) {
  const ref = useEdgeFlip<HTMLDivElement>();
  return (
    <div className="menu menu-submenu" role="menu" ref={ref}>
      {items.map((e, i) =>
        e.type === "separator" ? (
          <div key={i} className="menu-separator" role="separator" />
        ) : (
          <ItemRow
            key={i}
            entry={e as ItemEntry}
            activeRow={i === active}
            underline={underline}
            onHover={() => onHover(i)}
            onPick={() => onPick(e as ItemEntry)}
          />
        )
      )}
    </div>
  );
}

function MenuPanel({ items, active, subOpen, subActive, underline, onHover, onSubHover, onPick, className }: PanelProps) {
  const ref = useEdgeFlip<HTMLDivElement>();
  return (
    <div className={`menu ${className ?? ""}`} role="menu" ref={ref}>
      {items.map((e, i) => {
        if (e.type === "separator") return <div key={i} className="menu-separator" role="separator" />;
        if (e.type === "submenu") {
          const subItems = visible(e.items);
          return (
            <ItemRow key={i} entry={e} activeRow={i === active} underline={underline} onHover={() => onHover(i)}>
              {subOpen === i && (
                <SubPanel items={subItems} active={subActive} underline={underline} onHover={onSubHover} onPick={onPick} />
              )}
            </ItemRow>
          );
        }
        return (
          <ItemRow
            key={i}
            entry={e}
            activeRow={i === active}
            underline={underline}
            onHover={() => onHover(i)}
            onPick={() => onPick(e)}
          />
        );
      })}
    </div>
  );
}

function CompactPanel({ children }: { children: ReactNode }) {
  const ref = useEdgeFlip<HTMLDivElement>();
  return (
    <div className="menu menu-compact" role="menu" ref={ref}>
      {children}
    </div>
  );
}

/** Narrow windows: one overflow button with every menu as a labelled group. */
function CompactMenu({ menus, onPick }: { menus: MenuDef[]; onPick: (entry: ItemEntry) => void }) {
  const [open, setOpen] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const pick = (entry: ItemEntry) => {
    setOpen(false);
    onPick(entry);
  };

  return (
    <div className="menubar menubar-compact" ref={ref}>
      <div className={`menubar-menu ${open ? "is-open" : ""}`}>
        <button
          type="button"
          className="menubar-item menubar-overflow"
          aria-label="Menu"
          aria-haspopup="menu"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          <span aria-hidden="true">⋮</span>
        </button>
        {open && (
          <CompactPanel>
            {menus.map((m, mi) => (
              <Fragment key={m.id}>
                {mi > 0 && <div className="menu-separator" role="separator" />}
                <div className="menu-group-label">{plainLabel(m.label)}</div>
                {visible(m.items).map((e, i) => {
                  if (e.type === "separator") return null;
                  if (e.type === "submenu") {
                    const key = `${m.id}:${i}`;
                    return (
                      <ItemRow
                        key={key}
                        entry={e}
                        activeRow={false}
                        underline={false}
                        onHover={() => {}}
                        onPick={() => setExpanded((x) => (x === key ? null : key))}
                      >
                        {expanded === key && (
                          <div className="menu-inline-submenu" role="menu">
                            {visible(e.items).map((s, si) =>
                              s.type === "separator" ? null : (
                                <ItemRow
                                  key={si}
                                  entry={s as ItemEntry}
                                  activeRow={false}
                                  underline={false}
                                  onHover={() => {}}
                                  onPick={() => pick(s as ItemEntry)}
                                />
                              )
                            )}
                          </div>
                        )}
                      </ItemRow>
                    );
                  }
                  return (
                    <ItemRow key={i} entry={e} activeRow={false} underline={false} onHover={() => {}} onPick={() => pick(e)} />
                  );
                })}
              </Fragment>
            ))}
          </CompactPanel>
        )}
      </div>
    </div>
  );
}

export function MenuBar({ menus }: { menus: MenuDef[] }) {
  const shown = menus.filter((m) => !m.hidden);
  const compact = useMediaQuery(COMPACT_QUERY);
  const [open, setOpen] = useState<number | null>(null);
  /** Keyboard focus on the bar itself (after Alt) with no menu open. */
  const [barFocus, setBarFocus] = useState<number | null>(null);
  const [active, setActive] = useState(-1);
  const [subOpen, setSubOpen] = useState<number | null>(null);
  const [subActive, setSubActive] = useState(-1);
  const [altDown, setAltDown] = useState(false);
  /** Menu opened from the keyboard: show mnemonics, as Windows does. */
  const [viaKeyboard, setViaKeyboard] = useState(false);
  const ref = useRef<HTMLElement>(null);
  const altPending = useRef(false);

  const latest = useRef({ shown, compact, open, barFocus, active, subOpen, subActive });
  latest.current = { shown, compact, open, barFocus, active, subOpen, subActive };

  const close = useCallback(() => {
    setOpen(null);
    setBarFocus(null);
    setActive(-1);
    setSubOpen(null);
    setSubActive(-1);
  }, []);

  const openMenu = useCallback((index: number, highlightFirst: boolean) => {
    const items = visible(latest.current.shown[index]?.items ?? []);
    setOpen(index);
    setViaKeyboard(highlightFirst);
    setBarFocus(null);
    setSubOpen(null);
    setSubActive(-1);
    setActive(highlightFirst ? stepIndex(items, -1, 1) : -1);
  }, []);

  const pick = useCallback(
    (entry: ItemEntry) => {
      close();
      if (!entry.disabled) entry.onSelect();
    },
    [close]
  );

  // Accelerators, Alt handling and menu navigation.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const { shown, compact, open, barFocus, active, subOpen, subActive } = latest.current;

      if (e.key === "Alt") {
        altPending.current = !e.repeat;
        setAltDown(true);
        return;
      }
      altPending.current = false;

      // Alt+letter opens the matching menu.
      if (e.altKey && !e.ctrlKey && e.key.length === 1 && !compact) {
        const index = shown.findIndex((m) => mnemonic(m.label) === e.key.toLowerCase());
        if (index >= 0) {
          e.preventDefault();
          openMenu(index, true);
        }
        return;
      }

      if (open === null && barFocus === null) {
        for (const item of allItems(shown)) {
          if (item.shortcut && !item.nativeShortcut && !item.disabled && matchesShortcut(item.shortcut, e)) {
            e.preventDefault();
            item.onSelect();
            return;
          }
        }
        return;
      }

      const handled = () => {
        e.preventDefault();
        e.stopPropagation();
      };

      if (barFocus !== null) {
        if (e.key === "ArrowRight") setBarFocus((barFocus + 1) % shown.length);
        else if (e.key === "ArrowLeft") setBarFocus((barFocus - 1 + shown.length) % shown.length);
        else if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") openMenu(barFocus, true);
        else if (e.key === "Escape") close();
        else if (e.key.length === 1) {
          const index = shown.findIndex((m) => mnemonic(m.label) === e.key.toLowerCase());
          if (index >= 0) openMenu(index, true);
          else return;
        } else return;
        handled();
        return;
      }

      if (open === null) return;
      const items = visible(shown[open].items);
      const subItems = subOpen !== null && items[subOpen]?.type === "submenu"
        ? visible((items[subOpen] as SubmenuEntry).items)
        : null;

      switch (e.key) {
        case "Escape":
          if (subItems) {
            setSubOpen(null);
            setSubActive(-1);
          } else {
            close();
          }
          break;
        case "ArrowDown":
          if (subItems) setSubActive(stepIndex(subItems, subActive, 1));
          else setActive(stepIndex(items, active, 1));
          break;
        case "ArrowUp":
          if (subItems) setSubActive(stepIndex(subItems, subActive, -1));
          else setActive(stepIndex(items, active < 0 ? 0 : active, -1));
          break;
        case "ArrowRight":
          if (!subItems && items[active]?.type === "submenu" && !items[active].disabled) {
            const sub = visible((items[active] as SubmenuEntry).items);
            setSubOpen(active);
            setSubActive(stepIndex(sub, -1, 1));
          } else {
            openMenu((open + 1) % shown.length, true);
          }
          break;
        case "ArrowLeft":
          if (subItems) {
            setSubOpen(null);
            setSubActive(-1);
          } else {
            openMenu((open - 1 + shown.length) % shown.length, true);
          }
          break;
        case "Enter":
        case " ": {
          const target = subItems ? subItems[subActive] : items[active];
          if (!target || target.type === "separator" || target.disabled) break;
          if (target.type === "submenu") {
            const sub = visible(target.items);
            setSubOpen(active);
            setSubActive(stepIndex(sub, -1, 1));
          } else {
            pick(target);
          }
          break;
        }
        default: {
          // Item mnemonic (or first letter) picks within the open menu.
          if (e.key.length !== 1) return;
          const list = subItems ?? items;
          const letter = e.key.toLowerCase();
          const index = list.findIndex(
            (x) => x.type !== "separator" && !x.disabled && (mnemonic(x.label) ?? plainLabel(x.label)[0]?.toLowerCase()) === letter
          );
          if (index < 0) return;
          const target = list[index];
          if (target.type === "submenu") {
            setActive(index);
            setSubOpen(index);
            setSubActive(stepIndex(visible(target.items), -1, 1));
          } else if (target.type !== "separator") {
            pick(target);
          }
        }
      }
      handled();
    };

    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key !== "Alt") return;
      setAltDown(false);
      if (!altPending.current) return;
      altPending.current = false;
      e.preventDefault();
      const { compact, open, barFocus } = latest.current;
      if (compact) return;
      if (open !== null || barFocus !== null) close();
      else setBarFocus(0);
    };

    const onBlur = () => {
      setAltDown(false);
      altPending.current = false;
    };

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", onBlur);
    };
  }, [close, openMenu, pick]);

  // Click outside closes.
  useEffect(() => {
    if (open === null && barFocus === null) return;
    const onDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) close();
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open, barFocus, close]);

  if (compact) return <CompactMenu menus={shown} onPick={pick} />;

  const underline = altDown || barFocus !== null || (open !== null && viaKeyboard);

  return (
    <nav className={`menubar ${open !== null ? "is-engaged" : ""}`} role="menubar" aria-label="Application menu" ref={ref}>
      {shown.map((m, i) => (
        <div key={m.id} className={`menubar-menu ${open === i ? "is-open" : ""}`}>
          <button
            type="button"
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={open === i}
            tabIndex={-1}
            className={`menubar-item ${open === i ? "is-open" : ""} ${barFocus === i ? "is-focused" : ""}`}
            onPointerDown={(e) => {
              if (e.button > 0) return;
              e.preventDefault();
              if (open === i) close();
              else openMenu(i, false);
            }}
            onPointerEnter={() => {
              if (open !== null && open !== i) openMenu(i, false);
            }}
          >
            <span className="menubar-label">
              <Label text={m.label} underline={underline} />
            </span>
          </button>
          {open === i && (
            <MenuPanel
              items={visible(m.items)}
              active={active}
              subOpen={subOpen}
              subActive={subActive}
              underline={underline}
              onHover={(index) => {
                setActive(index);
                const entry = visible(m.items)[index];
                if (entry?.type === "submenu" && !entry.disabled) {
                  setSubOpen(index);
                  setSubActive(-1);
                } else {
                  setSubOpen(null);
                }
              }}
              onSubHover={setSubActive}
              onPick={pick}
            />
          )}
        </div>
      ))}
    </nav>
  );
}
