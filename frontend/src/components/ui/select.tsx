import type { SelectHTMLAttributes } from "react";

export function Select(props: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      {...props}
      className={[
        "h-10 w-full rounded-md border border-[#cbd5ce] bg-white px-3 text-sm outline-none",
        "focus:border-[#1f6f50]",
        props.className ?? ""
      ].join(" ")}
    />
  );
}
