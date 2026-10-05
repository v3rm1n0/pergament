import * as React from "react";
import * as TabsPrimitive from "@radix-ui/react-tabs";
import { cn } from "@/lib/utils";

export const Tabs = TabsPrimitive.Root;

export const TabsList = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.List>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.List
    ref={ref}
    className={cn("flex items-end gap-1 overflow-x-auto border-b border-line bg-surface px-4", className)}
    {...props}
  />
));
TabsList.displayName = "TabsList";

/** Upper-case tab with a brand underline when active. */
export const TabsTrigger = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Trigger>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Trigger
    ref={ref}
    className={cn(
      "relative shrink-0 px-3.5 pb-3 pt-3.5 text-[1.05rem] uppercase tracking-wide text-fg/80 hover:text-fg",
      "data-[state=active]:font-semibold data-[state=active]:text-accent",
      "after:absolute after:inset-x-0 after:bottom-0 after:h-[3px] after:bg-accent after:opacity-0 data-[state=active]:after:opacity-100",
      className,
    )}
    {...props}
  />
));
TabsTrigger.displayName = "TabsTrigger";

export const TabsContent = TabsPrimitive.Content;
