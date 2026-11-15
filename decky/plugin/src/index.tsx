import { addEventListener, definePlugin, removeEventListener, routerHook } from "@decky/api";
import { staticClasses } from "@decky/ui";
import { FaGamepad } from "react-icons/fa";
import { ButtonsPage } from "./ButtonsPage";
import { QuickPanel, ROUTE } from "./QuickPanel";
import { onWriteDone, onWriteStarted } from "./store";

export default definePlugin(() => {
  routerHook.addRoute(ROUTE, ButtonsPage);
  const started = addEventListener("write_started", onWriteStarted);
  const done = addEventListener("write_done", onWriteDone);

  return {
    name: "Flugdiggi",
    titleView: <div className={staticClasses.Title}>Flugdiggi</div>,
    content: <QuickPanel />,
    icon: <FaGamepad />,
    onDismount() {
      routerHook.removeRoute(ROUTE);
      removeEventListener("write_started", started);
      removeEventListener("write_done", done);
    },
  };
});
