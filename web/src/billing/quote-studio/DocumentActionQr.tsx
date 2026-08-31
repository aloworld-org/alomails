import { QRCodeSVG } from "qrcode.react";

export function DocumentActionQr({ value, label }: { value: string; label: string }) {
  return (
    <aside className="mt-10 flex items-center gap-4 border-t border-[var(--quote-border)] pt-5 text-[var(--quote-text)]">
      <div className="shrink-0 rounded-lg bg-white p-2 ring-1 ring-black/10">
        <QRCodeSVG
          value={value}
          size={88}
          bgColor="#ffffff"
          fgColor="#102a43"
          level="M"
          marginSize={0}
          title={label}
        />
      </div>
      <div>
        <p className="text-sm font-semibold leading-tight">{label}</p>
      </div>
    </aside>
  );
}
