import { useEffect, useRef, useState } from "react";
import styles from "./Pwa.module.css";

const DISMISS_KEY = "oxide:pwa-install-dismissed";

interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

function getDismissed(): boolean {
  try {
    return localStorage.getItem(DISMISS_KEY) === "1";
  } catch {
    return false;
  }
}

function setDismissed(): void {
  try {
    localStorage.setItem(DISMISS_KEY, "1");
  } catch {
    /* ignore */
  }
}

export function InstallPrompt() {
  const [deferred, setDeferred] = useState<BeforeInstallPromptEvent | null>(
    null,
  );
  const [visible, setVisible] = useState(false);
  const used = useRef(false);

  useEffect(() => {
    const onPrompt = (e: Event) => {
      e.preventDefault();
      if (used.current) return;
      const evt = e as BeforeInstallPromptEvent;
      setDeferred(evt);
      setVisible(!getDismissed());
    };
    const onInstalled = () => {
      try {
        localStorage.removeItem(DISMISS_KEY);
      } catch {
        /* ignore */
      }
      setVisible(false);
      setDeferred(null);
    };
    window.addEventListener("beforeinstallprompt", onPrompt);
    window.addEventListener("appinstalled", onInstalled);
    return () => {
      window.removeEventListener("beforeinstallprompt", onPrompt);
      window.removeEventListener("appinstalled", onInstalled);
    };
  }, []);

  const install = async () => {
    if (!deferred || used.current) return;
    used.current = true;
    try {
      await deferred.prompt();
    } catch {
      setVisible(false);
      setDeferred(null);
      return;
    }
    const choice = await deferred.userChoice;
    if (choice.outcome === "dismissed") setDismissed();
    setVisible(false);
    setDeferred(null);
  };

  const dismiss = () => {
    setDismissed();
    setVisible(false);
  };

  if (!visible || !deferred) return null;

  return (
    <div className={styles.prompt} role="dialog" aria-label="Install Oxide">
      <div className={styles.text}>
        <strong>Install Oxide</strong>
        <span>Add to your home screen for quick access.</span>
      </div>
      <button className={styles.install} onClick={install}>
        Install
      </button>
      <button className={styles.dismiss} onClick={dismiss} aria-label="Dismiss">
        ✕
      </button>
    </div>
  );
}
