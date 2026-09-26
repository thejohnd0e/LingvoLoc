export function targetForDetectedLanguage(
  detectedLanguage: string,
  primaryLanguage: string,
  secondaryLanguage: string,
): string {
  if (!primaryLanguage || !secondaryLanguage) return 'en';
  return detectedLanguage === primaryLanguage
    ? secondaryLanguage
    : primaryLanguage;
}
