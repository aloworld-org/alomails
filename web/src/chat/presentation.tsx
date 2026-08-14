import type { ReactNode } from "react";

import { strings } from "../i18n";
import type { ChannelSummary, Message, Proposal } from "./types";

export interface Nameable {
  handle: string;
  label: string;
  agent: boolean;
}

export const channelLabel = (channel: ChannelSummary): string =>
  channel.name ?? channel.counterpart ?? strings.chatDirectMessage;

/** A sidebar is a list of people, not addresses. Until the identity directory
 * carries profile names, turn the stable address local-part into a calm human
 * label while keeping the full address in the conversation header. */
export const directMessageName = (channel: ChannelSummary): string => {
  if (channel.kind !== "dm" || channel.counterpart === null) return channelLabel(channel);
  const local = channel.counterpart.split("@", 1)[0]?.replace(/[._-]+/g, " ").trim();
  if (!local) return channel.counterpart;
  return local.replace(/\b\p{L}/gu, (letter) => letter.toLocaleUpperCase());
};

export function personName(email: string | null, id: string): string {
  if (email === null) return id;
  const at = email.indexOf("@");
  return at > 0 ? email.slice(0, at) : email;
}

export function continues(message: Message, before: Message | undefined): boolean {
  if (before === undefined || before.author !== message.author) return false;
  if (before.authorKind !== message.authorKind) return false;
  if (before.proposal !== null || before.attachments.length > 0) return false;
  const gap = new Date(message.createdAt).getTime() - new Date(before.createdAt).getTime();
  return gap >= 0 && gap < 5 * 60_000;
}

export function dayOf(iso: string): string {
  const date = new Date(iso);
  const today = new Date();
  const same = (left: Date, right: Date) => left.toDateString() === right.toDateString();
  if (same(date, today)) return strings.chatToday;
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (same(date, yesterday)) return strings.chatYesterday;
  return date.toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
    ...(date.getFullYear() === today.getFullYear() ? {} : { year: "numeric" }),
  });
}

export const shortTime = (iso: string): string =>
  new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });

export function timeOf(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime())
    ? ""
    : at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

export function withHandlesMarked(body: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const pattern = /(^|[\s([{"'])(@[A-Za-z0-9._%+-]+(?:@[A-Za-z0-9.-]+)?)/g;
  let at = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(body)) !== null) {
    const start = match.index + match[1]!.length;
    if (start > at) parts.push(body.slice(at, start));
    parts.push(<span key={start} className="font-semibold text-accent">{match[2]}</span>);
    at = start + match[2]!.length;
  }
  if (at < body.length) parts.push(body.slice(at));
  return parts;
}

export function mentionAt(value: string, caret: number): { start: number; token: string } | null {
  const upto = value.slice(0, caret);
  const at = upto.lastIndexOf("@");
  if (at < 0) return null;
  const before = at === 0 ? " " : upto[at - 1]!;
  if (!/[\s([{"']/.test(before)) return null;
  const token = upto.slice(at + 1);
  return /\s/.test(token) ? null : { start: at, token: token.toLowerCase() };
}

export function candidatesFor(token: string, all: Nameable[]): Nameable[] {
  const matching = all.filter((nameable) => nameable.handle.startsWith(token));
  return [...matching.filter((item) => item.agent), ...matching.filter((item) => !item.agent)].slice(0, 6);
}

export function standingOf(
  proposal: Proposal,
  me: string | null,
): { standing?: { decidable: false; reason: string } } {
  if (proposal.state !== "pending") {
    return { standing: { decidable: false, reason: strings.chatProposalSettled(proposal.state) } };
  }
  return proposal.askedBy === me
    ? {}
    : { standing: { decidable: false, reason: strings.chatProposalNotYours } };
}
