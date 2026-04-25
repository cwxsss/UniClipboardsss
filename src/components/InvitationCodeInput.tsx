import { type ComponentProps } from 'react'
import { InputOTP, InputOTPGroup, InputOTPSeparator, InputOTPSlot } from '@/components/ui/input-otp'
import { cn } from '@/lib/utils'

/**
 * Invitation code length matches the rendezvous-issued code shape currently
 * used in tests: 4 alphanumeric chars + dash + 4 alphanumeric chars (total
 * 8 OTP slots). The dash is purely visual — the wire form keeps it as
 * `XXXX-XXXX` but `InputOTPSeparator` renders it independently of the
 * underlying value.
 */
export const INVITATION_CODE_LENGTH = 8

/** Allowed input chars: A-Z + 0-9 (auto-uppercased on entry). */
const ALLOWED_CHARS = /[A-Z0-9]/

type Props = Omit<
  ComponentProps<typeof InputOTP>,
  'maxLength' | 'pattern' | 'value' | 'onChange' | 'render' | 'children'
> & {
  value: string
  onChange: (value: string) => void
  disabled?: boolean
  /** Renders the slots with `aria-invalid="true"` styling. */
  invalid?: boolean
  /** Optional class for the surrounding container. */
  className?: string
}

/**
 * 8-slot OTP-style input for setup invitation codes.
 *
 * - auto upper-cases keystrokes and pasted text
 * - accepts only A-Z and 0-9; strips whitespace and dashes from paste
 * - splits 4-4 with a visual separator that does not consume input
 */
export function InvitationCodeInput({
  value,
  onChange,
  disabled,
  invalid,
  className,
  ...rest
}: Props) {
  const handleChange = (next: string) => {
    const cleaned = next
      .toUpperCase()
      .split('')
      .filter(c => ALLOWED_CHARS.test(c))
      .join('')
      .slice(0, INVITATION_CODE_LENGTH)
    onChange(cleaned)
  }

  return (
    <InputOTP
      maxLength={INVITATION_CODE_LENGTH}
      value={value}
      onChange={handleChange}
      disabled={disabled}
      aria-invalid={invalid || undefined}
      containerClassName={cn('justify-center gap-2', className)}
      {...rest}
    >
      <InputOTPGroup>
        <InputOTPSlot index={0} className="size-10 text-base font-mono uppercase" />
        <InputOTPSlot index={1} className="size-10 text-base font-mono uppercase" />
        <InputOTPSlot index={2} className="size-10 text-base font-mono uppercase" />
        <InputOTPSlot index={3} className="size-10 text-base font-mono uppercase" />
      </InputOTPGroup>
      <InputOTPSeparator />
      <InputOTPGroup>
        <InputOTPSlot index={4} className="size-10 text-base font-mono uppercase" />
        <InputOTPSlot index={5} className="size-10 text-base font-mono uppercase" />
        <InputOTPSlot index={6} className="size-10 text-base font-mono uppercase" />
        <InputOTPSlot index={7} className="size-10 text-base font-mono uppercase" />
      </InputOTPGroup>
    </InputOTP>
  )
}

/** Format a raw 8-char code as `XXXX-XXXX` for display surfaces. */
export function formatInvitationCode(raw: string): string {
  const clean = raw.toUpperCase().replace(/[^A-Z0-9]/g, '')
  if (clean.length <= 4) return clean
  return `${clean.slice(0, 4)}-${clean.slice(4, INVITATION_CODE_LENGTH)}`
}
