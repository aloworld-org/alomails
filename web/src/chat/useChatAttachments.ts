import { useState } from "react";

import { useJmapClient } from "../jmap";
import type { DriveNodeDto } from "../jmap/types";
import { strings } from "../i18n";
import { chatMessage } from "./api";

export const CHAT_ATTACHMENTS_MAX = 10;

export function useChatAttachments(onError: (message: string | null) => void) {
  const client = useJmapClient();
  const [staged, setStaged] = useState<DriveNodeDto[]>([]);
  const [picking, setPicking] = useState(false);
  const [dropping, setDropping] = useState(false);

  async function shareDropped(files: FileList) {
    setDropping(false);
    if (files.length === 0) return;
    onError(null);
    try {
      for (const file of Array.from(files).slice(0, CHAT_ATTACHMENTS_MAX)) {
        const id = await client.driveUpload(null, null, file);
        const node = await client.driveNode(id);
        if (node === null) continue;
        setStaged((held) => held.length >= CHAT_ATTACHMENTS_MAX || held.some((item) => item.id === node.id) ? held : [...held, node]);
      }
    } catch (failure) { onError(chatMessage(failure, strings.chatAttachFailed)); }
  }

  function mergePicked(files: DriveNodeDto[]) {
    setPicking(false);
    setStaged((held) => {
      const merged = [...held];
      for (const file of files) if (!merged.some((item) => item.id === file.id) && merged.length < CHAT_ATTACHMENTS_MAX) merged.push(file);
      return merged;
    });
  }

  return { staged, setStaged, picking, setPicking, dropping, setDropping, shareDropped, mergePicked };
}
