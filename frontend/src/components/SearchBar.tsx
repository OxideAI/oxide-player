import { useEffect, useRef, useState } from "react";
import styles from "./SearchBar.module.css";

interface SearchBarProps {
  open: boolean;
  onClose: () => void;
  onSearch: (query: string) => void;
}

export function SearchBar({ open, onClose, onSearch }: SearchBarProps) {
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setValue("");
    const t = setTimeout(() => inputRef.current?.focus(), 0);
    return () => clearTimeout(t);
  }, [open]);

  if (!open) return null;

  const close = () => {
    setValue("");
    onClose();
  };

  const submit = () => {
    const q = value.trim();
    if (!q) {
      close();
      return;
    }
    onSearch(q);
    setValue("");
    onClose();
  };

  return (
    <div className={styles.scrim} onClick={close}>
      <div className={styles.bar} onClick={(e) => e.stopPropagation()}>
        <svg
          className={styles.icon}
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.7"
          strokeLinecap="round"
        >
          <circle cx="11" cy="11" r="6.5" />
          <path d="M20 20l-3.6-3.6" />
        </svg>
        <input
          ref={inputRef}
          className={styles.input}
          placeholder="Search tracks, albums, artists…"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.preventDefault();
              close();
            } else if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
        />
        <span className={styles.hint}>Esc to close · Enter to search</span>
      </div>
    </div>
  );
}
