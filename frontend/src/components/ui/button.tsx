import type { ButtonHTMLAttributes } from "react";

export function Button(props: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      {...props}
      className={[
        "inline-flex h-10 items-center justify-center rounded-md bg-[#1f6f50] px-4 text-sm font-medium text-white",
        "disabled:cursor-not-allowed disabled:bg-[#9aa8a0]",
        props.className ?? ""
      ].join(" ")}
    />
  );
}
