// What somebody sees when they reach an app their administrator switched off
// (migration 0208).
//
// The rail leaves the entry out, so this is reached by a typed URL, an old
// bookmark, or a link a colleague sent — all of which are ordinary things to
// do, none of which deserve a broken screen. It says what happened, and it
// says who can undo it, because the person reading it cannot.
//
// Deliberately not an error. Nothing went wrong: a decision was made about
// this account, and the sentence should read like one.
import { Link } from "react-router-dom";
import { Lock } from "lucide-react";

import { strings } from "../i18n";
import { surface } from "../product";
import styles from "./ComingSoon.module.css";

export function ModuleSwitchedOff({ title }: { title?: string }) {
  return (
    <div className={styles.wrap}>
      <div className={styles.badge}>
        <Lock strokeWidth={1.5} />
      </div>
      <h2 className={styles.title}>{title ?? strings.accessModuleOff}</h2>
      <p className={styles.body}>{strings.accessModuleOffHint}</p>
      <Link to={surface.defaultPath} className={styles.action}>
        {strings.accessBackHome}
      </Link>
    </div>
  );
}
