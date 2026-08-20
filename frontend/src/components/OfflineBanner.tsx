import { useEffect, useState } from "react";
import styles from "./Pwa.module.css";

export function OfflineBanner() {
  const [offline, setOffline] = useState(
    typeof navigator !== "undefined" ? !navigator.onLine : false,
  );

  useEffect(() => {
    const onOffline = () => setOffline(true);
    const onOnline = () => setOffline(false);
    window.addEventListener("offline", onOffline);
    window.addEventListener("online", onOnline);
    return () => {
      window.removeEventListener("offline", onOffline);
      window.removeEventListener("online", onOnline);
    };
  }, []);

  if (!offline) return null;

  return (
    <div className={styles.offline} role="status">
      <span className={styles.dot} />
      You are offline. The app shell loads from cache, but library data and
      playback need a connection.
    </div>
  );
}
