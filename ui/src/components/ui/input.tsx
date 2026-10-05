import * as React from "react";
import { cn } from "@/lib/utils";

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      className={cn(
        "h-10 rounded border border-line bg-surface px-3 text-sm text-fg outline-none placeholder:text-muted focus:border-accent",
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = "Input";
