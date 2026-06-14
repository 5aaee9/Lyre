import type { InputHTMLAttributes } from "react";

export function Switch(props: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...props}
      type="checkbox"
      className={["h-5 w-5 accent-[#1f6f50]", props.className ?? ""].join(" ")}
    />
  );
}
