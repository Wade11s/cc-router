import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  // 与 styles.css 的 .btn 同一套手绘外形: 墨线 + 不规则圆角, 悬停歪一下并落出硬投影
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-(--r-sketch) border-[1.8px] border-(--stroke) text-sm font-medium transition-[transform,box-shadow,background-color] duration-150 hover:-translate-x-px hover:-translate-y-px hover:-rotate-[0.6deg] hover:shadow-[3px_3px_0_var(--fill-oat)] active:translate-x-px active:translate-y-px active:rotate-0 active:shadow-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-(--accent) disabled:pointer-events-none disabled:opacity-50 motion-reduce:hover:transform-none",
  {
    variants: {
      variant: {
        default: "bg-(--accent) font-semibold text-[#141413] hover:shadow-[3px_3px_0_var(--stroke)]",
        destructive: "rounded-(--r-sketch-alt) bg-(--fill-coral) font-semibold text-(--ink) hover:shadow-[3px_3px_0_var(--err)]",
        outline: "bg-(--surface) text-(--ink)",
        secondary: "bg-(--surface-3) text-(--ink)",
        ghost: "border-transparent text-(--ink-2) hover:bg-(--surface-3) hover:text-(--ink) hover:shadow-none hover:transform-none",
        link: "border-transparent text-(--accent-ink) underline-offset-4 hover:underline hover:shadow-none hover:transform-none",
      },
      size: {
        default: "h-9 px-4 py-2",
        sm: "h-8 px-3 text-xs",
        lg: "h-10 px-6",
        icon: "h-9 w-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { buttonVariants };
