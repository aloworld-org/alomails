// Public surface of the Campaigns area (alo Campaigns, ADR 0044). The product
// surface mounts the module; everything else is internal to the folder.
export { CampaignsModule } from "./CampaignsModule";
export { CampaignsApi, campaignsMessage, useCampaignsApi } from "./api";
export type {
  AudienceMember,
  CampaignConsent,
  CampaignSegment,
  CampaignSuppression,
  SegmentConditions,
  SegmentTally,
} from "./types";
