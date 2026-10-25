import { useSyncExternalStore } from "react";
import { useSession } from "./react";

export function useJobLedger() {
  const { jobs } = useSession();
  return useSyncExternalStore(jobs.subscribe, jobs.getSnapshot);
}
