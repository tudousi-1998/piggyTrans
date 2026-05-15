import SettingsApp from "./SettingsApp";
import OverlayApp from "./OverlayApp";

export default function App({ windowLabel }: { windowLabel: string }) {
  if (windowLabel === "overlay") {
    return <OverlayApp />;
  }
  return <SettingsApp />;
}
