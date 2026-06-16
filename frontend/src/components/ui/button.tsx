import type { ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "destructive" | "outline" | "ghost";
};

const variants: Record<NonNullable<ButtonProps["variant"]>, string> = {
  default: "bg-neutral-950 text-neutral-50 shadow hover:bg-neutral-950/90",
  destructive: "bg-red-600 text-neutral-50 shadow-sm hover:bg-red-600/90",
  outline: "border border-neutral-200 bg-white shadow-sm hover:bg-neutral-100 hover:text-neutral-900",
  ghost: "hover:bg-neutral-100 hover:text-neutral-900"
};

export function Button({ className, variant = "default", ...props }: ButtonProps) {
  return (
    <button
      {...props}
      className={cn(
        "inline-flex h-9 items-center justify-center whitespace-nowrap rounded-md px-4 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-neutral-950 disabled:pointer-events-none disabled:opacity-50",
        variants[variant],
        className
      )}
    />
  );
}
