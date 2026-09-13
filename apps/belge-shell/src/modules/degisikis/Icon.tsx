import type { ReactNode } from "react";

/** Yalnız gerçekten çizilen simgeler. Kullanılmayan dokuz tanesi kaldırıldı. */
export type IconName = "swap" | "chevron-up" | "chevron-down" | "upload" | "file";

const PATHS: Record<IconName, ReactNode> = {
  swap: <><path d="M7 7h11l-3-3m3 3-3 3" /><path d="M17 17H6l3 3m-3-3 3-3" /></>,
  "chevron-up": <path d="m6 15 6-6 6 6" />,
  "chevron-down": <path d="m6 9 6 6 6-6" />,
  upload: <><path d="M12 16V4m0 0L7 9m5-5 5 5" /><path d="M5 14v5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-5" /></>,
  file: <><path d="M6 3h8l4 4v14H6z" /><path d="M14 3v5h4M9 13h6M9 17h6" /></>,
};

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.3"
      vectorEffect="non-scaling-stroke"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}
