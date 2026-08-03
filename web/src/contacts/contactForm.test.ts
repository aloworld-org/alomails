import { describe, expect, test } from "vitest";

import { toDraft, toForm, type FormState } from "./ContactsModal";
import type { Contact } from "../jmap";

describe("toForm", () => {
  test("maps a stored contact into controlled form state", () => {
    const c: Contact = {
      id: "x1",
      name: "Alice Martin",
      firstName: "Alice",
      lastName: "Martin",
      emails: [{ kind: "work", value: "alice@example.eu" }],
      phones: [],
      organization: "Example",
      jobTitle: null,
      notes: null,
    };
    const f = toForm(c);
    expect(f.firstName).toBe("Alice");
    expect(f.displayName).toBe("Alice Martin");
    expect(f.emails).toEqual([{ kind: "work", value: "alice@example.eu" }]);
    // Nulls become empty strings (controlled inputs), and an empty email
    // list still yields one blank row to type into.
    expect(f.jobTitle).toBe("");
    expect(toForm({ ...c, emails: [] }).emails).toEqual([{ kind: null, value: "" }]);
  });
});

describe("toDraft", () => {
  const base: FormState = {
    firstName: "",
    lastName: "",
    displayName: "",
    emails: [{ kind: null, value: "" }],
    phones: [],
    organization: "",
    jobTitle: "",
    notes: "",
  };

  test("drops blank email/phone rows and trims values", () => {
    const draft = toDraft({
      ...base,
      emails: [
        { kind: "work", value: "  a@b.eu " },
        { kind: null, value: "   " }, // blank → dropped
      ],
      phones: [{ kind: "mobile", value: "+33 1" }],
    });
    expect(draft.emails).toEqual([{ kind: "work", value: "a@b.eu" }]);
    expect(draft.phones).toEqual([{ kind: "mobile", value: "+33 1" }]);
  });

  test("blank scalar fields become null", () => {
    const draft = toDraft({ ...base, organization: "  ", jobTitle: "Dev" });
    expect(draft.organization).toBeNull();
    expect(draft.jobTitle).toBe("Dev");
  });

  test("omits `name` when empty so the server derives it", () => {
    // Omitted (not undefined) under exactOptionalPropertyTypes.
    const draft = toDraft(base);
    expect("name" in draft).toBe(false);
    const named = toDraft({ ...base, displayName: "  Bob  " });
    expect(named.name).toBe("Bob");
  });
});
