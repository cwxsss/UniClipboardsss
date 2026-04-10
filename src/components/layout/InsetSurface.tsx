import React from 'react'
import { usePlatform } from '@/hooks/usePlatform'
import { cn } from '@/lib/utils'

type InsetSurfaceProps = React.HTMLAttributes<HTMLDivElement>

/**
 * InsetSurface: A refined content container with an 'elevated sheet' aesthetic.
 *
 * Design Features:
 * - Large 24px top-left rounding for a modern, friendly feel.
 * - Subtle inner borders (ring) and glossy top highlight for depth.
 * - Platform-specific optimizations (Mica-like effect on Windows).
 * - Smooth transitions for theme changes.
 */
const InsetSurface: React.FC<InsetSurfaceProps> = ({ className, children, ...props }) => {
  const { isWindows } = usePlatform()

  return (
    <div
      className={cn(
        'relative flex min-h-0 flex-1 flex-col overflow-hidden transition-all duration-300',
        'rounded-[20px] bg-background',
        // Light mode: Shadowless for a clean, flat look
        // Dark mode: Keep the rich layered shadows for depth
        'shadow-none dark:shadow-[-8px_0_24px_-12px_rgba(0,0,0,0.2),0_4px_24px_-4px_rgba(0,0,0,0.1),0_8px_16px_-8px_rgba(0,0,0,0.1)]',
        isWindows ? 'bg-background/94 backdrop-blur-xl' : 'bg-background',
        // High-precision ring for edge definition in light mode
        'ring-1 ring-black/[0.05] dark:ring-white/[0.08]',
        className
      )}
      {...props}
    >
      {/* Refined Glass Highlight - Top Edge */}
      <div
        className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-white/25 to-transparent dark:via-white/10"
        aria-hidden="true"
      />

      {/* Subtle Inset Shadow - Edges */}
      <div
        className="pointer-events-none absolute inset-y-0 left-0 w-px bg-gradient-to-b from-transparent via-black/[0.02] to-transparent dark:via-white/[0.02]"
        aria-hidden="true"
      />

      {/* Adaptive Background Texture (SVG Noise) */}
      <div
        className="pointer-events-none absolute inset-0 opacity-[0.015] dark:opacity-[0.03] mix-blend-overlay"
        style={{
          backgroundImage: `url("data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3C%3Cfilter id='noiseFilter'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.65' numOctaves='3' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23noiseFilter)'/%3E%3C/svg%3E")`,
        }}
        aria-hidden="true"
      />

      {/* Main Content Area */}
      <div className="relative z-10 flex min-h-0 flex-1 flex-col">{children}</div>

      {/* Subtle Corner Glow */}
      <div
        className="pointer-events-none absolute left-0 top-0 h-24 w-24 rounded-tl-[20px] bg-gradient-to-br from-primary/5 to-transparent blur-2xl dark:from-primary/10"
        aria-hidden="true"
      />
    </div>
  )
}

export default InsetSurface
