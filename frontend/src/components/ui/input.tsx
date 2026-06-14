import type { InputHTMLAttributes } from "react";

export function Input(props: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...props}
      className={[
        "h-10 w-full rounded-md border border-[#cbd5ce] bg-white px-3 text-sm outline-none",
        "focus:border-[#1f6f50]",
        props.className ?? ""
      ].join(" ")}
    />
  );
}
