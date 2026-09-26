import * as React from "react";
import { cn } from "@/lib/utils";

export type TextareaProps = React.TextareaHTMLAttributes<HTMLTextAreaElement>;

export const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className, ...props }, ref) => (
    <textarea
      className={cn(
        "flex min-h-[60px] w-full rounded-(--r-card-alt) border-[1.5px] border-(--line-2) bg-(--surface) px-3 py-2 text-sm transition-[border-color,box-shadow] placeholder:text-muted-foreground hover:border-(--ink-4) focus-visible:border-(--stroke) focus-visible:shadow-[3px_3px_0_var(--fill-butter)] focus-visible:outline-hidden disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      ref={ref}
      {...props}
    />
  ),
);
Textarea.displayName = "Textarea";
