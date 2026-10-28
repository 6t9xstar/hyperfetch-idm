import { useEffect } from "react";
import { useStore } from "./store";
import { Sidebar } from "./components/Sidebar";
import { Toolbar } from "./components/Toolbar";
import { StatsBar } from "./components/StatsBar";
import { DownloadTable } from "./components/DownloadTable";
import { AddDownloadDialog } from "./components/AddDownloadDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { CapturesDialog } from "./components/CapturesDialog";
import { GrabberDialog } from "./components/GrabberDialog";
import { Toasts } from "./components/Toasts";
import { ClassicShell } from "./components/classic/ClassicShell";
import { Logo } from "./components/Icons";

function Splash() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4">
      <Logo className="h-14 w-14 animate-pulse" />
      <div className="text-center">
        <p className="text-sm font-semibold tracking-tight text-slate-200">HyperFetch</p>
        <p className="mt-1 text-[11px] text-slate-500">Starting transfer engine…</p>
      </div>
    </div>
  );
}

export default function App() {
  const initialized = useStore((state) => state.initialized);
  const init = useStore((state) => state.init);
  const openAdd = useStore((state) => state.openAdd);
  const theme = useStore((state) => state.settings?.theme ?? "modern");

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    document.documentElement.dataset.theme =
      theme === "classic" ? "classic" : "modern";
  }, [theme]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "n") {
        event.preventDefault();
        openAdd();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openAdd]);

  if (!initialized) {
    return (
      <div className="h-full">
        <Splash />
      </div>
    );
  }

  const classic = theme === "classic";

  return (
    <div className={classic ? "h-full" : "flex h-full overflow-hidden"}>
      {classic ? (
        <ClassicShell />
      ) : (
        <>
          <Sidebar />
          <main className="flex min-w-0 flex-1 flex-col bg-ink-950/40">
            <Toolbar />
            <StatsBar />
            <DownloadTable />
          </main>
        </>
      )}
      <AddDownloadDialog />
      <SettingsDialog />
      <CapturesDialog />
      <GrabberDialog />
      <Toasts />
    </div>
  );
}
