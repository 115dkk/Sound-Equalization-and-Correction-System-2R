// Buttons. One `primary` per view (the step's next action); peers are
// `secondary`; low-weight actions in dense places are `quiet`; actions that
// remove data are `danger`. The label names the effect.
import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import { Icon, Spinner, type IconName } from "./Icon";

/** `go` moves on to the next step from a result, beside actions that redo it; it reads apart from the primary by colour. */
export type ButtonVariant = "primary" | "secondary" | "quiet" | "danger" | "go";

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant | undefined;
  size?: "md" | "sm" | undefined;
  icon?: IconName | undefined;
  iconEnd?: IconName | undefined;
  /** Work in progress: shows a spinner, keeps the label, and blocks clicks. */
  busy?: boolean | undefined;
  /** No press scale (for buttons pressed in quick succession). */
  static?: boolean | undefined;
  children?: ReactNode;
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", icon, iconEnd, busy = false, static: isStatic = false, className, children, disabled, type, ...rest },
  ref,
) {
  const classes = [
    "ui-button",
    `ui-button--${variant}`,
    size === "sm" && "ui-button--sm",
    isStatic && "ui-button--static",
    !children && "ui-button--icon-only",
    icon && children && "ui-button--lead",
    iconEnd && children && "ui-button--trail",
    className,
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={classes}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      {...rest}
    >
      {busy ? <Spinner size={16} /> : icon ? <Icon name={icon} size={size === "sm" ? 16 : 18} /> : null}
      {children !== undefined && <span className="ui-button__label">{children}</span>}
      {iconEnd && !busy && <Icon name={iconEnd} size={size === "sm" ? 16 : 18} />}
    </button>
  );
});

/** A square button with only an icon; the label is required for screen readers and the tooltip. */
export const IconButton = forwardRef<HTMLButtonElement, Omit<ButtonProps, "children" | "icon"> & { icon: IconName; label: string }>(
  function IconButton({ icon, label, variant = "quiet", ...rest }, ref) {
    return <Button ref={ref} variant={variant} icon={icon} aria-label={label} {...rest} />;
  },
);
