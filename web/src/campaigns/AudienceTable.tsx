// The people a question selects — **all** of them, mailable or not (C1.5).
//
// Filtering the excluded out of this table would be the easy version and the
// wrong one: somebody who unsubscribed is usually still a customer the tenant
// invoices, and a colleague who cannot see them cannot check whether the count
// is right. So the status column is the point of the table, not decoration.
//
// It renders what the server sent. `mailable` and the reason are both fields on
// the wire, so this file never works out whether somebody may be mailed — a
// second opinion about that is exactly the bug ADR 0044 §2 exists to prevent.
import { Badge, Table, TableEmpty, Td, Th } from "../ds";
import { strings } from "../i18n";
import { exclusionLabel, personLabel, sourceLabel } from "./format";
import type { AudienceMember } from "./types";
import styles from "./CampaignsModule.module.css";

export function AudienceTable({ people }: { people: AudienceMember[] }) {
  return (
    <Table label={strings.campaignsTableLabel} stickyHeader>
      <thead>
        <tr>
          <Th>{strings.campaignsColPerson}</Th>
          <Th>{strings.campaignsColCountry}</Th>
          <Th>{strings.campaignsColKnownFrom}</Th>
          <Th>{strings.campaignsColStatus}</Th>
        </tr>
      </thead>
      <tbody>
        {people.length === 0 ? (
          <TableEmpty cols={4}>{strings.campaignsNoMatches}</TableEmpty>
        ) : (
          people.map((person) => (
            <tr key={person.address}>
              <Td>
                <span className={styles.person}>{personLabel(person.name, person.address)}</span>
                {person.name !== null && person.name.trim() !== "" && (
                  <span className={styles.address}>{person.address}</span>
                )}
              </Td>
              <Td>{person.country ?? "—"}</Td>
              <Td>{person.sources.map(sourceLabel).join(", ")}</Td>
              <Td>
                {person.mailable ? (
                  <Badge tone="success">{strings.campaignsWillBeMailed}</Badge>
                ) : (
                  <Badge tone="neutral">
                    {exclusionLabel(person.exclusionReason ?? "")}
                  </Badge>
                )}
              </Td>
            </tr>
          ))
        )}
      </tbody>
    </Table>
  );
}
