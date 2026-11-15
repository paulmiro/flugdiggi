import { callable } from "@decky/api";

export interface Failure {
  ok: false;
  error: { kind: string; message: string };
}
type Ok<T> = { ok: true } & T;
export type Reply<T> = Ok<T> | Failure;

export type Buttons = Record<string, string>;

export interface Meta {
  buttons: { id: string; label: string; extra: boolean }[];
  targets: { id: string; label: string }[];
  slots: number;
}
export interface Device {
  port: string;
  product: string;
  connection: string;
  connection_guessed: boolean;
}
export interface Status {
  info: { firmware: string; battery: string; connection: string; mac: string };
  active_slot: number;
}
export interface SlotButtons {
  slot: number;
  name: string;
  buttons: Buttons;
  data_version: number;
}
export interface Written extends SlotButtons {
  outcome: "written" | "unchanged";
}

export const meta = callable<[], Reply<Meta>>("meta");
export const listDevices = callable<[], Reply<{ devices: Device[] }>>("list_devices");
export const status = callable<[port: string], Reply<Status>>("status");
export const readButtons = callable<[port: string, slot: number], Reply<SlotButtons>>("read_buttons");
export const activate = callable<[port: string, slot: number], Reply<{ active_slot: number }>>("activate");
export const writeButtons = callable<
  [port: string, slot: number, buttons: Buttons, expectedVersion: number],
  void
>("write_buttons");
