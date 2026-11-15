import { ButtonItem, DropdownItem, Navigation, PanelSection, PanelSectionRow, Spinner } from "@decky/ui";
import { useEffect } from "react";
import { isDirty, refresh, scan, selectDevice, selectSlot, useStore } from "./store";

export const ROUTE = "/flugdiggi";

export function QuickPanel() {
  const s = useStore();
  useEffect(() => {
    // Only lists devices; reading the controller needs the button below.
    void scan();
  }, []);

  return (
    <PanelSection title="Controller">
      {s.devices.length > 1 && (
        <PanelSectionRow>
          <DropdownItem
            label="Device"
            rgOptions={s.devices.map((d) => ({ data: d.port, label: `${d.product} (${d.connection}, ${d.port})` }))}
            selectedOption={s.port}
            disabled={!!s.busy}
            onChange={(o) => selectDevice(o.data)}
          />
        </PanelSectionRow>
      )}
      {s.status && (
        <PanelSectionRow>
          <div>
            Battery {s.status.info.battery} · {s.status.info.connection} · firmware {s.status.info.firmware}
          </div>
        </PanelSectionRow>
      )}
      {s.status && s.meta && (
        <PanelSectionRow>
          <DropdownItem
            label="Active slot"
            rgOptions={Array.from({ length: s.meta.slots }, (_, i) => ({
              data: i,
              label: i === s.slot && s.slotName ? `${i + 1}: ${s.slotName}` : `${i + 1}`,
            }))}
            selectedOption={s.status.active_slot}
            disabled={!!s.busy || isDirty(s)}
            onChange={(o) => void selectSlot(o.data)}
          />
        </PanelSectionRow>
      )}
      <PanelSectionRow>
        <ButtonItem
          layout="below"
          disabled={!s.port}
          onClick={() => {
            Navigation.Navigate(ROUTE);
            Navigation.CloseSideMenus();
          }}
        >
          Edit button mapping…
        </ButtonItem>
      </PanelSectionRow>
      {s.busy && (
        <PanelSectionRow>
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <Spinner style={{ width: "1.5em", height: "1.5em" }} /> {s.busy}
          </div>
        </PanelSectionRow>
      )}
      {s.error && (
        <PanelSectionRow>
          <div style={{ color: "#ff6b6b" }}>{s.error}</div>
        </PanelSectionRow>
      )}
      <PanelSectionRow>
        <ButtonItem layout="below" disabled={!s.port || !!s.busy || isDirty(s)} onClick={() => void refresh()}>
          {s.loaded ? "Read again" : "Read from controller"}
        </ButtonItem>
      </PanelSectionRow>
      {!s.loaded && s.port && (
        <PanelSectionRow>
          <div>Reading briefly disconnects the controller.</div>
        </PanelSectionRow>
      )}
    </PanelSection>
  );
}
