import { withHandlesMarked } from "./presentation";
import { renderBody } from "./richText";

export function RichMessageBody({ body }: { body: string }) {
  return <>{renderBody(body, withHandlesMarked)}</>;
}
