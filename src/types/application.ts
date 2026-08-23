export interface Application {
  id: string;
  name: string;
  path: string;
  bundleId?: string;
  icon?: string;
  source: string;
  launchCount: number;
  lastLaunchTime?: number;
}
