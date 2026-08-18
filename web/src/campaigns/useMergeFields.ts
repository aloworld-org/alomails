// The vocabulary a letter can personalise with (alo Campaigns, ADR 0044, wave
// C3.6).
//
// Its own read, and its own file, because it changes for a reason nothing else
// on the screen shares: a server release, not a letter and not a recipient. It
// is fetched once per mount and never again.
//
// **The names come from the server and the words do not.** A client-side copy
// of the list goes stale the day a merge field is added; a server-side copy of
// the *labels* arrives in English whatever language the reader set. So the wire
// carries `first_name` and this catalogue carries "First name", in three
// languages — which is also why an unknown name falls through to itself rather
// than to an apology: a field this build has no words for is still a field a
// writer can use.
import { useEffect, useState } from "react";

import { useCampaignsApi } from "./api";

/** Every field a campaign can personalise with, in the order a composer offers
 *  them. Empty until the read lands, and empty for good if it fails — the
 *  guide is help, and a screen that showed an error banner because its help
 *  did not load would be worse than one that quietly showed no help. */
export function useMergeFields(): string[] {
  const api = useCampaignsApi();
  const [fields, setFields] = useState<string[]>([]);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const vocabulary = await api.mergeFields();
        if (live) setFields(vocabulary);
      } catch {
        if (live) setFields([]);
      }
    })();
    return () => {
      live = false;
    };
  }, [api]);

  return fields;
}
