// Public surface of the HR area. The product surface mounts the module;
// nothing outside reaches into the views, the drawer or the API client.
export { HrModule } from "./HrModule";

// The waiting-approvals badge lives in the rail rather than in the module,
// because an inbox you can only see by opening it is an inbox that keeps people
// waiting. It is declared by the workspace product surface, so the standalone
// mail product — which has no HR, Finance or Projects — never renders or
// imports it.
export { ApprovalsWidget } from "./ApprovalsWidget";
