// The account control at the foot of the rail: the user's avatar, opening a
// small popover with who they're signed in as and a sign-out action.
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { BookUser, Download, Globe, LogOut, Settings, Shield } from "lucide-react";

import { strings, LOCALES, getLocale, setLocale, type Locale } from "../i18n";
import { Avatar } from "../ds";
import { useAuth } from "../auth";
import { useJmapClient } from "../jmap";
import { ContactsModal } from "../contacts";
import { ImportModal } from "../importer";
import { SettingsModal } from "./SettingsModal";
import styles from "./UserMenu.module.css";

export function UserMenu() {
  const { identity, signOut } = useAuth();
  const client = useJmapClient();
  const [open, setOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [contactsOpen, setContactsOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [isAdmin, setIsAdmin] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let live = true;
    void client
      .isAdmin()
      .then((ok) => {
        if (live) setIsAdmin(ok);
      })
      .catch(() => {
        // not admin / unavailable → link stays hidden
      });
    return () => {
      live = false;
    };
  }, [client]);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: PointerEvent) {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const name = identity?.name ?? "";
  const email = identity?.email ?? "";

  return (
    <div className={styles.wrap} ref={ref}>
      <button
        type="button"
        className={styles.trigger}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={strings.userMenu}
      >
        <Avatar name={name} email={email} size="md" />
      </button>

      {open && (
        <div className={styles.menu} role="menu">
          <div className={styles.identity}>
            <span className={styles.who}>{strings.signedInAs}</span>
            <span className={styles.name}>{name}</span>
            <span className={styles.email}>{email}</span>
          </div>
          {isAdmin && (
            <Link
              to="/admin"
              className={styles.item}
              role="menuitem"
              onClick={() => setOpen(false)}
            >
              <Shield size={16} />
              <span>{strings.adminOpen}</span>
            </Link>
          )}
          <button
            type="button"
            className={styles.item}
            role="menuitem"
            onClick={() => {
              setOpen(false);
              setContactsOpen(true);
            }}
          >
            <BookUser size={16} />
            <span>{strings.contactsOpen}</span>
          </button>
          <button
            type="button"
            className={styles.item}
            role="menuitem"
            onClick={() => {
              setOpen(false);
              setImportOpen(true);
            }}
          >
            <Download size={16} />
            <span>{strings.importOpen}</span>
          </button>
          <button
            type="button"
            className={styles.item}
            role="menuitem"
            onClick={() => {
              setOpen(false);
              setSettingsOpen(true);
            }}
          >
            <Settings size={16} />
            <span>{strings.settingsOpen}</span>
          </button>
          <div className={styles.menuDivider} />
          <div className={styles.languageRow}>
            <Globe size={16} />
            <span>{strings.language}</span>
            <select
              className={styles.languageSelect}
              aria-label={strings.language}
              value={getLocale()}
              onChange={(e) => setLocale(e.target.value as Locale)}
            >
              {LOCALES.map((l) => (
                <option key={l.code} value={l.code}>
                  {l.label}
                </option>
              ))}
            </select>
          </div>
          <div className={styles.menuDivider} />
          <button
            type="button"
            className={styles.item}
            role="menuitem"
            onClick={() => {
              setOpen(false);
              void signOut();
            }}
          >
            <LogOut size={16} />
            <span>{strings.signOut}</span>
          </button>
        </div>
      )}
      {settingsOpen && (
        <SettingsModal isAdmin={isAdmin} onClose={() => setSettingsOpen(false)} />
      )}
      {contactsOpen && <ContactsModal onClose={() => setContactsOpen(false)} />}
      {importOpen && <ImportModal onClose={() => setImportOpen(false)} />}
    </div>
  );
}
