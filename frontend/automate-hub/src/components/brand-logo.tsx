interface BrandLogoProps {
  className?: string;
}

export function BrandLogo({ className = "h-7 w-7" }: BrandLogoProps) {
  return (
    <img
      src="/favicon.ico"
      alt="Greentic"
      className={`shrink-0 object-contain ${className}`}
      draggable={false}
    />
  );
}
