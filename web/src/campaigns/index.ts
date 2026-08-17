// Public surface of the Campaigns area (alo Campaigns, ADR 0044). The product
// surface mounts the module; everything else is internal to the folder.
export { CampaignsModule } from "./CampaignsModule";
// The public landing page at the end of an unsubscribe link (ADR 0044 §3). Not
// part of the module: it is mounted on its own route, outside the shell and
// outside RequireAuth, because the person reading it has no account.
export { UnsubscribeView } from "./UnsubscribeView";
export { CampaignsApi, campaignsMessage, useCampaignsApi } from "./api";
export type {
  AudienceMember,
  CampaignConsent,
  CampaignSegment,
  CampaignSuppression,
  SegmentConditions,
  SegmentTally,
} from "./types";
