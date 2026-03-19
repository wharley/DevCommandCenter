import React, { useMemo } from "react";
import { Navigate, useParams } from "react-router-dom";

export default function ProjectWorkspaceIndexRedirect() {
  const { id: projectId } = useParams<{ id: string }>();

  const target = useMemo(() => {
    if (!projectId) return "/";
    const showPipeline = typeof localStorage !== "undefined" && localStorage.getItem("dcc:showPipelineTab") === "true";
    const saved = typeof localStorage !== "undefined" ? localStorage.getItem(`dcc:project:${projectId}:workspace`) : null;
    const workspace =
      saved === "agents" ? "agents" : showPipeline && saved === "pipeline" ? "pipeline" : "agents";
    return `/project/${projectId}/${workspace}`;
  }, [projectId]);

  return <Navigate to={target} replace />;
}
