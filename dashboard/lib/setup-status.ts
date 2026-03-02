import { syncServerUrl } from "@/lib/server-api";

type SetupStatusResponse = {
  needs_setup?: boolean;
};

export async function fetchSetupStatus(): Promise<{ needsSetup: boolean }> {
  try {
    const response = await fetch(syncServerUrl("/auth/setup-status"), {
      headers: {
        Accept: "application/json",
      },
      cache: "no-store",
    });

    if (!response.ok) {
      return { needsSetup: false };
    }

    const body = (await response.json()) as SetupStatusResponse;
    return { needsSetup: Boolean(body.needs_setup) };
  } catch {
    return { needsSetup: false };
  }
}
