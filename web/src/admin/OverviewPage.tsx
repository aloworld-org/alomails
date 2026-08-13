// Admin — Overview. The tenant at a glance: how many users, how much storage,
// whether mail is set up to deliver, and whether AI is on — with a card into
// each management area. Read-only; it composes the same endpoints the detail
// pages use, so there is no new backend and nothing here can drift from them.
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Globe, ShieldCheck, Sparkles, UserRound, Users } from "lucide-react";

import { strings } from "../i18n";
import { Card, Spinner, cx } from "../ds";
import { useJmapClient } from "../jmap";
import { formatBytes } from "../mail/format";
import styles from "./admin.module.css";

interface Stats {
  users: number;
  storageBytes: number;
  deliverPass: number;
  deliverTotal: number;
  aiEnabled: boolean;
}

export function OverviewPage() {
  const client = useJmapClient();
  const [stats, setStats] = useState<Stats | null>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      // Each is best-effort; a failed call leaves its tile as a dash rather
      // than failing the whole dashboard.
      const [users, security, ai] = await Promise.all([
        client.listUsers().catch(() => null),
        client.securityChecks().catch(() => null),
        client.aiEnabled().catch(() => false),
      ]);
      if (!live) return;
      setStats({
        users: users?.length ?? 0,
        storageBytes: (users ?? []).reduce((sum, u) => sum + u.storageBytes, 0),
        deliverPass: security?.checks.filter((c) => c.status === "pass").length ?? 0,
        deliverTotal: security?.checks.length ?? 0,
        aiEnabled: ai,
      });
    })();
    return () => {
      live = false;
    };
  }, [client]);

  const sections = [
    { to: "/admin/users", Icon: UserRound, name: strings.adminUsers, desc: strings.adminUsersIntro },
    { to: "/admin/groups", Icon: Users, name: strings.adminGroups, desc: strings.adminGroupsIntro },
    { to: "/admin/domains", Icon: Globe, name: strings.adminDomains, desc: strings.adminDomainsIntro },
    { to: "/admin/security", Icon: ShieldCheck, name: strings.adminSecurity, desc: strings.adminSecurityIntro },
    { to: "/admin/ai", Icon: Sparkles, name: strings.adminAiProviders, desc: strings.adminAiIntro },
  ];

  return (
    <div className={styles.page}>
      <header className={styles.pageHead}>
        <div>
          <h1>{strings.adminOverview}</h1>
          <p className={styles.pageIntro}>{strings.adminOverviewIntro}</p>
        </div>
      </header>

      {stats === null ? (
        <div className={styles.state}>
          <Spinner size={22} />
        </div>
      ) : (
        <>
          <div className={styles.statRow}>
            <div className={styles.stat}>
              <div className={styles.statValue}>{stats.users}</div>
              <div className={styles.statLabel}>{strings.overviewUsers}</div>
            </div>
            <div className={styles.stat}>
              <div className={styles.statValue}>{formatBytes(stats.storageBytes)}</div>
              <div className={styles.statLabel}>{strings.overviewStorage}</div>
            </div>
            <div className={styles.stat}>
              <div className={styles.statValue}>
                {stats.deliverTotal > 0 ? `${stats.deliverPass}/${stats.deliverTotal}` : "—"}
              </div>
              <div className={styles.statLabel}>{strings.overviewDeliverability}</div>
              {stats.deliverTotal > 0 && (
                <div
                  className={cx(
                    styles.statHint,
                    stats.deliverPass === stats.deliverTotal ? styles.chkPass : styles.chkWarn,
                  )}
                >
                  {stats.deliverPass === stats.deliverTotal
                    ? strings.overviewDeliverOk
                    : strings.overviewDeliverAttention}
                </div>
              )}
            </div>
            <div className={styles.stat}>
              <div className={styles.statValue}>
                {stats.aiEnabled ? strings.overviewOn : strings.overviewOff}
              </div>
              <div className={styles.statLabel}>{strings.overviewAi}</div>
            </div>
          </div>

          <h2 className={styles.sectionTitle}>{strings.overviewManage}</h2>
          <div className={styles.cardGrid}>
            {sections.map((s) => (
              /* `Card interactive` wrapping a link, which is what that variant
                 documents itself as being for. The link covers the card rather
                 than containing it, so its accessible name is the section —
                 "Users & mailboxes" — instead of the heading and the sentence
                 under it read as one run-on phrase. */
              <Card key={s.to} interactive className={cx(styles.cardRow, styles.cardLink)}>
                <span className={styles.cardIcon}>
                  <s.Icon size={20} strokeWidth={1.75} />
                </span>
                <div className={styles.cardText}>
                  <Link to={s.to} className={styles.cardCover}>
                    {s.name}
                  </Link>
                  <div className={styles.cardDesc}>{s.desc}</div>
                </div>
              </Card>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
