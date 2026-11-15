import { useEffect, useState } from "react";
import {
  Buttons,
  Device,
  Meta,
  Reply,
  Status,
  Written,
  activate,
  listDevices,
  meta,
  readButtons,
  status,
  writeButtons,
} from "./api";

// Module-level: the QAM component unmounts whenever the menu closes, and unsaved edits must survive that.
export interface State {
  meta?: Meta;
  devices: Device[];
  port?: string;
  status?: Status;
  slot: number;
  slotName: string;
  saved: Buttons;
  edited: Buttons;
  dataVersion: number;
  loaded: boolean;
  busy?: string;
  error?: string;
}

let state: State = { devices: [], slot: 0, slotName: "", saved: {}, edited: {}, dataVersion: 0, loaded: false };
const listeners = new Set<() => void>();

function set(patch: Partial<State>) {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

export function useStore(): State {
  const [, rerender] = useState(0);
  useEffect(() => {
    const l = () => rerender((n) => n + 1);
    listeners.add(l);
    return () => {
      listeners.delete(l);
    };
  }, []);
  return state;
}

export const isDirty = (s: State) => JSON.stringify(sorted(s.saved)) !== JSON.stringify(sorted(s.edited));
const sorted = (b: Buttons) => Object.entries(b).sort(([a], [c]) => a.localeCompare(c));

function fail<T>(r: Reply<T>): r is Extract<Reply<T>, { ok: false }> {
  if (r.ok) return false;
  set({ busy: undefined, error: r.error.message });
  return true;
}

async function loadSlot(port: string, slot: number) {
  const r = await readButtons(port, slot);
  if (fail(r)) return;
  set({
    slot: r.slot,
    slotName: r.name,
    saved: r.buttons,
    edited: { ...r.buttons },
    dataVersion: r.data_version,
    loaded: true,
    busy: undefined,
    error: undefined,
  });
}

/**
 * Lists the connected devices. Claims no device, so xpad stays attached; safe to call automatically.
 * Returns the selected port, if any.
 */
export async function scan(): Promise<string | undefined> {
  if (!state.meta) {
    const m = await meta();
    if (fail(m)) return undefined;
    set({ meta: m });
  }
  const d = await listDevices();
  if (fail(d)) return undefined;
  const port = d.devices.find((x) => x.port === state.port)?.port ?? d.devices[0]?.port;
  set({ devices: d.devices, port, error: port ? state.error : "No controller found" });
  return port;
}

/**
 * Reads the device info and the active slot's mapping. This detaches xpad for a moment, so it only runs on
 * an explicit user action, never automatically.
 */
export async function refresh() {
  set({ busy: "Reading the controller…", error: undefined });
  const port = await scan();
  if (!port) {
    set({ busy: undefined, status: undefined, loaded: false });
    return;
  }
  const s = await status(port);
  if (fail(s)) return;
  set({ status: s });
  await loadSlot(port, s.active_slot);
}

export function selectDevice(port: string) {
  set({ port, loaded: false, status: undefined, error: undefined });
}

export async function selectSlot(slot: number) {
  if (!state.port) return;
  set({ busy: "Switching slot…", error: undefined });
  const a = await activate(state.port, slot);
  if (fail(a)) return;
  set({ status: state.status && { ...state.status, active_slot: a.active_slot } });
  await loadSlot(state.port, slot);
}

export function setButton(button: string, target: string) {
  const edited = { ...state.edited };
  if (target === "default") delete edited[button];
  else edited[button] = target;
  set({ edited });
}

export function revert() {
  set({ edited: { ...state.saved } });
}

export function save() {
  if (!state.port) return;
  set({ busy: "Saving… (about 7 s, the controller disconnects)", error: undefined });
  void writeButtons(state.port, state.slot, state.edited, state.dataVersion);
}

export function onWriteStarted() {
  set({ busy: "Saving… (about 7 s, the controller disconnects)" });
}

/** Shows what the firmware stored, not what we sent. */
export function onWriteDone(slot: number, r: Reply<Written>) {
  if (fail(r)) return;
  set({
    slot,
    saved: r.buttons,
    edited: { ...r.buttons },
    dataVersion: r.data_version,
    busy: undefined,
    error: undefined,
  });
}
