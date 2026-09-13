/**
 * Ortak ilkeller.
 *
 * Bunlar bir "design system framework" değil. Envanterde aynı görünümün
 * ekranlarda satır içi stille tekrar tekrar yazıldığı görüldü — bölüm başlığı
 * dört ayrı yerde aynı beş CSS özelliğiyle elle kuruluyordu. Buradaki her
 * bileşen gerçek bir tekrarın karşılığıdır; soyutlama gösterisi değildir.
 */

import type { ButtonHTMLAttributes, ReactNode } from "react";

/* ------------------------------------------------------------------ Button */

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "primary" | "quiet" | "danger";
};

export function Button({ variant = "default", className, ...rest }: ButtonProps) {
  const kind = variant === "default" ? "" : ` btn-${variant}`;
  return <button type="button" className={`btn${kind}${className ? ` ${className}` : ""}`} {...rest} />;
}

/* -------------------------------------------------------------- IconButton
   İkon-only kontrolde erişilebilir ad ZORUNLU: `label` hem tooltip hem
   aria-label olur. Tooltip tek başına ekran okuyucuya yetmez. */

type IconButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label"> & {
  label: string;
  pressed?: boolean;
  children: ReactNode;
};

export function IconButton({ label, pressed, className, children, ...rest }: IconButtonProps) {
  return (
    <button
      type="button"
      className={`icon-btn${className ? ` ${className}` : ""}`}
      title={label}
      aria-label={label}
      aria-pressed={pressed}
      {...rest}
    >
      {children}
    </button>
  );
}

/* --------------------------------------------------------------- Section */

export function Section({
  title,
  children,
  id,
}: {
  title?: string;
  children: ReactNode;
  id?: string;
}) {
  const headingId = id ? `${id}-baslik` : undefined;
  return (
    <section className="section" aria-labelledby={headingId}>
      {title ? (
        <h2 className="section-head" id={headingId}>
          {title}
        </h2>
      ) : null}
      {children}
    </section>
  );
}

/* ------------------------------------------------------------ EmptyState
   Dört ekran aynı bilgi mimarisini — ne oldu, tek cümle, (varsa) tek eylem —
   dört ayrı biçimde çiziyordu: iki farklı dikey hizalama, iki farklı optik
   konum, dört CSS bloğu. Tek yüzey kaldı. Kart, çerçeve, ikon ve çizim yok;
   konum optik üst-orta (kalan alanın %38'i üstte). */

export function EmptyState({
  title,
  note,
  meta,
  action,
  alert = false,
}: {
  title: string;
  /** Tek cümle. Ne olduğu ya da ne yapılabileceği. */
  note?: ReactNode;
  /** Sessiz künye satırı — gerçek bir sayı varsa. */
  meta?: ReactNode;
  /** Tek kurtarma/başlangıç eylemi. */
  action?: ReactNode;
  /** Hata durumu: ekran okuyucuya duyurulur. */
  alert?: boolean;
}) {
  return (
    <div className="empty" role={alert ? "alert" : undefined}>
      <div className="empty-body">
        <h1 className="empty-title">{title}</h1>
        {note ? <p className="empty-note">{note}</p> : null}
        {meta ? <p className="empty-meta">{meta}</p> : null}
        {action ? <div className="empty-action">{action}</div> : null}
      </div>
    </div>
  );
}

/* ----------------------------------------------------------------- Status
   Anlam yalnız renkle verilmez: her tonun bir işareti var. */

const MARK = { info: "", error: "✕", success: "✓", busy: "" } as const;

export function Status({
  tone = "info",
  children,
  live = "polite",
}: {
  tone?: "info" | "error" | "success" | "busy";
  children: ReactNode;
  live?: "polite" | "assertive" | "off";
}) {
  return (
    <p
      className="status"
      data-tone={tone === "busy" ? undefined : tone}
      role={tone === "error" ? "alert" : "status"}
      aria-live={tone === "error" ? undefined : live}
    >
      {tone === "busy" ? <span className="spinner" aria-hidden="true" /> : null}
      {MARK[tone] ? (
        <span className="status-mark" aria-hidden="true">
          {MARK[tone]}
        </span>
      ) : null}
      <span className="status-text">{children}</span>
    </p>
  );
}

/* ------------------------------------------------------------------- Pill
   Tür rozeti: hairline çerçeve, tek renk, BÜYÜK HARF. Renkli rozet değil. */

export function Pill({ children, className }: { children: ReactNode; className?: string }) {
  return <span className={`pill${className ? ` ${className}` : ""}`}>{children}</span>;
}

/* ------------------------------------------------------------------ Field */

export function Field({
  label,
  hint,
  htmlFor,
  children,
}: {
  label: string;
  hint?: string;
  htmlFor?: string;
  children: ReactNode;
}) {
  return (
    <div className="field">
      <label className="field-label" htmlFor={htmlFor}>
        {label}
        {hint ? <span>{hint}</span> : null}
      </label>
      {children}
    </div>
  );
}
