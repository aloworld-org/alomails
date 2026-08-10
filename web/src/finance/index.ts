// Public surface of the Finance area. The product surface mounts the module;
// nothing outside reaches into the views, the dialogs or the API client.
export { FinanceModule } from "./FinanceModule";

// The one deliberate exception, and it is narrow on purpose: the shell's agent
// receipt (B4.14a) lets a person accept or decline a suggested category where
// they are already looking, and it must do it through THIS module's client —
// one session, one error shape, one set of rules — rather than by growing a
// second client for two routes. Nothing else of the API surface is exported.
export { financeMessage, useFinanceApi } from "./api";
