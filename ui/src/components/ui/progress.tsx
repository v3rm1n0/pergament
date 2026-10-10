import * as ProgressPrimitive from "@radix-ui/react-progress";
import { cn } from "@/lib/utils";

/** Determinate when `value` is a number, indeterminate (pulsing) otherwise. */
export function Progress({ value, className, label }: { value: number | null; className?: string; label?: string }) {
  return (
    <ProgressPrimitive.Root
      value={value ?? undefined}
      aria-label={label}
      className={cn("relative h-1.5 w-full overflow-hidden rounded bg-line", className)}
    >
      <ProgressPrimitive.Indicator
        className={cn("h-full bg-accent transition-transform", value == null && "w-1/3 animate-pulse motion-reduce:animate-none")}
        style={value != null ? { transform: `translateX(-${100 - value}%)` } : undefined}
      />
    </ProgressPrimitive.Root>
  );
}
