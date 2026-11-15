import {
  ConfirmModal,
  DialogButton,
  DropdownItem,
  Focusable,
  PanelSection,
  SidebarNavigation,
  Spinner,
  showModal,
} from "@decky/ui";
import { isDirty, refresh, revert, save, setButton, useStore } from "./store";

const LOCKED = ["macro", "keyboard"];

function Buttons() {
  const s = useStore();
  const dirty = isDirty(s);

  if (!s.loaded || !s.meta) {
    return (
      <PanelSection title="Buttons">
        <DialogButton disabled={!!s.busy} onClick={() => void refresh()}>
          Read from controller
        </DialogButton>
        <div style={{ margin: "8px 0" }}>Reading briefly disconnects the controller.</div>
        {s.busy && <Spinner style={{ width: "1.5em", height: "1.5em" }} />}
        {s.error && <div style={{ color: "#ff6b6b" }}>{s.error}</div>}
      </PanelSection>
    );
  }
  const meta = s.meta;

  const confirmSave = () =>
    showModal(
      <ConfirmModal
        strTitle="Save button mapping?"
        strDescription="Saving takes about 7 seconds. The controller disconnects while it is written, so games see it vanish."
        strOKButtonText="Save"
        onOK={save}
      />,
    );

  return (
    <div>
      <PanelSection title={s.slotName ? `Slot ${s.slot + 1}: ${s.slotName}` : `Slot ${s.slot + 1}`}>
        {meta.buttons.map((b) => {
          const current = s.edited[b.id];
          const locked = current !== undefined && LOCKED.includes(current);
          const options = [
            { data: "default", label: b.extra ? "Nothing" : "Default" },
            ...meta.targets.filter((t) => t.id !== b.id).map((t) => ({ data: t.id, label: t.label })),
            ...(locked ? [{ data: current, label: current === "macro" ? "Macro" : "Keyboard" }] : []),
          ];
          return (
            <DropdownItem
              key={b.id}
              label={b.label}
              rgOptions={options}
              selectedOption={current ?? "default"}
              disabled={!!s.busy || locked}
              onChange={(o) => setButton(b.id, o.data)}
            />
          );
        })}
      </PanelSection>
      <Focusable style={{ display: "flex", gap: "8px", alignItems: "center", margin: "16px 0" }}>
        <DialogButton disabled={!dirty || !!s.busy} onClick={confirmSave}>
          Save
        </DialogButton>
        <DialogButton disabled={!dirty || !!s.busy} onClick={revert}>
          Revert
        </DialogButton>
        <DialogButton disabled={!!s.busy} onClick={() => void refresh()}>
          Read again
        </DialogButton>
        {s.busy && (
          <span style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <Spinner style={{ width: "1.5em", height: "1.5em" }} /> {s.busy}
          </span>
        )}
      </Focusable>
      {s.error && <div style={{ color: "#ff6b6b" }}>{s.error}</div>}
    </div>
  );
}

export function ButtonsPage() {
  return <SidebarNavigation pages={[{ title: "Buttons", content: <Buttons />, route: "/flugdiggi/buttons" }]} />;
}
