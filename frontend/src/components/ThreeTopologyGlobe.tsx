import React, { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { useKamish } from '../context/KamishContext';
import type { RegionId } from '../types';
import { Shield, Globe, RotateCw, Zap } from 'lucide-react';

// Convert lat/long to 3D Cartesian coordinates on sphere of radius R
function latLongToVector3(lat: number, lon: number, radius: number): THREE.Vector3 {
  const phi = (90 - lat) * (Math.PI / 180);
  const theta = (lon + 180) * (Math.PI / 180);
  const x = -(radius * Math.sin(phi) * Math.cos(theta));
  const z = radius * Math.sin(phi) * Math.sin(theta);
  const y = radius * Math.cos(phi);
  return new THREE.Vector3(x, y, z);
}

// Generate quadratic bezier curve points elevated above the sphere
function createElevatedCurve(v1: THREE.Vector3, v2: THREE.Vector3, elevation: number): THREE.QuadraticBezierCurve3 {
  const mid = new THREE.Vector3().addVectors(v1, v2).multiplyScalar(0.5);
  const distance = v1.distanceTo(v2);
  const normal = mid.clone().normalize();
  const controlPoint = mid.add(normal.multiplyScalar(distance * elevation));
  return new THREE.QuadraticBezierCurve3(v1, controlPoint, v2);
}

export interface ThreeTopologyGlobeProps {
  isGlobalBackground?: boolean;
  activeTab?: 'overview' | 'grafana' | 'audit' | 'chaos';
  showHud?: boolean;
  setShowHud?: (show: boolean) => void;
}

export const ThreeTopologyGlobe: React.FC<ThreeTopologyGlobeProps> = ({
  isGlobalBackground = true,
  activeTab = 'overview',
  showHud = true,
  setShowHud,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const { nodes, wanDelayMs, isPartitioned, adaptiveRoutingActive, isShadowMonarchMode } = useKamish();
  const [selectedRegion, setSelectedRegion] = useState<RegionId>('EU_WEST');
  const [autoRotate, setAutoRotate] = useState<boolean>(true);

  // References to mutable Three.js objects
  const sceneRef = useRef<THREE.Scene | null>(null);
  const cameraRef = useRef<THREE.PerspectiveCamera | null>(null);
  const rendererRef = useRef<THREE.WebGLRenderer | null>(null);
  const globeGroupRef = useRef<THREE.Group | null>(null);
  const animationFrameRef = useRef<number | null>(null);

  // Particles along arcs
  const arcParticlesRef = useRef<
    {
      curve: THREE.QuadraticBezierCurve3;
      mesh: THREE.Mesh;
      t: number;
      speed: number;
      linkId: string;
    }[]
  >([]);

  // Links curves mapping
  const curvesRef = useRef<{ [key: string]: { curve: THREE.QuadraticBezierCurve3; line: THREE.Line } }>({});
  // 3D Epoch Lease Orbit Halo ref
  const leaseHaloRef = useRef<THREE.Mesh | null>(null);

  // Interaction & orientation tracking refs
  const autoRotateRef = useRef<boolean>(autoRotate);
  autoRotateRef.current = autoRotate;
  const isDraggingRef = useRef<boolean>(false);
  const previousPositionRef = useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  const targetRotationRef = useRef<{ x: number; y: number } | null>(null);

  // Manual button rotation & zoom helpers
  const rotateGlobe = (deltaY: number, deltaX: number = 0) => {
    if (!globeGroupRef.current) return;
    targetRotationRef.current = null;
    globeGroupRef.current.rotation.y += deltaY;
    globeGroupRef.current.rotation.x = Math.max(-1.3, Math.min(1.3, globeGroupRef.current.rotation.x + deltaX));
  };

  const zoomCamera = (deltaZ: number) => {
    if (!cameraRef.current) return;
    cameraRef.current.position.z = Math.max(12, Math.min(42, cameraRef.current.position.z + deltaZ));
  };

  const resetView = () => {
    if (!globeGroupRef.current || !cameraRef.current) return;
    targetRotationRef.current = null;
    globeGroupRef.current.rotation.set(0.25, 1.1, 0);
    cameraRef.current.position.set(0, 8, 24);
  };

  // Auto focus globe orientation on selected region
  useEffect(() => {
    if (selectedRegion === 'EU_WEST') {
      targetRotationRef.current = { x: 0.35, y: -0.15 };
    } else if (selectedRegion === 'US_EAST') {
      targetRotationRef.current = { x: 0.25, y: 1.35 };
    } else if (selectedRegion === 'AP_SOUTH') {
      targetRotationRef.current = { x: 0.15, y: -1.25 };
    }
  }, [selectedRegion]);

  useEffect(() => {
    if (!containerRef.current) return;
    const container = containerRef.current;
    const width = isGlobalBackground ? window.innerWidth : (container.clientWidth || 450);
    const height = isGlobalBackground ? window.innerHeight : (container.clientHeight || 450);

    // 1. Scene setup
    const scene = new THREE.Scene();
    sceneRef.current = scene;

    // 2. Camera setup
    const camera = new THREE.PerspectiveCamera(45, width / height, 0.1, 1000);
    camera.position.set(0, isGlobalBackground ? 4 : 8, isGlobalBackground ? 26 : 24);
    cameraRef.current = camera;

    // 3. Renderer setup
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setSize(width, height);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    rendererRef.current = renderer;

    container.innerHTML = '';
    container.appendChild(renderer.domElement);

    // 4. Lighting
    // 4. Lighting - Solo Leveling Electric Violet & Shadow Mana
    const ambientLight = new THREE.AmbientLight(0x23143d, 1.8);
    scene.add(ambientLight);

    const dirLight1 = new THREE.DirectionalLight(0x9d4edd, 2.5);
    dirLight1.position.set(20, 20, 20);
    scene.add(dirLight1);

    const dirLight2 = new THREE.DirectionalLight(0x3a86ff, 1.4);
    dirLight2.position.set(-20, -10, -20);
    scene.add(dirLight2);

    // 5. Starfield / Floating Mana Soul Embers
    const starGeometry = new THREE.BufferGeometry();
    const starCount = 400;
    const starPositions = new Float32Array(starCount * 3);
    for (let i = 0; i < starCount * 3; i += 3) {
      starPositions[i] = (Math.random() - 0.5) * 120;
      starPositions[i + 1] = (Math.random() - 0.5) * 120;
      starPositions[i + 2] = (Math.random() - 0.5) * 120;
    }
    starGeometry.setAttribute('position', new THREE.BufferAttribute(starPositions, 3));
    const starMaterial = new THREE.PointsMaterial({
      color: 0xc77dff,
      size: 0.9,
      transparent: true,
      opacity: 0.75,
    });
    const starField = new THREE.Points(starGeometry, starMaterial);
    scene.add(starField);

    // 6. Main Globe Group
    const globeGroup = new THREE.Group();
    scene.add(globeGroup);
    globeGroupRef.current = globeGroup;

    const GLOBE_RADIUS = 7.5;

    // Inner Abyssal Void Core (Solo Leveling Shadow Monarch darkness)
    const coreGeo = new THREE.SphereGeometry(GLOBE_RADIUS, 48, 48);
    const coreMat = new THREE.MeshPhongMaterial({
      color: 0x05030a,
      emissive: 0x0a0518,
      specular: 0x7b2cbf,
      shininess: 35,
      transparent: true,
      opacity: 0.96,
    });
    const coreMesh = new THREE.Mesh(coreGeo, coreMat);
    globeGroup.add(coreMesh);

    // Ethereal Shadow Mana Grid
    const wireMat = new THREE.MeshBasicMaterial({
      color: 0x7b2cbf,
      wireframe: true,
      transparent: true,
      opacity: 0.16,
    });
    const wireMesh = new THREE.Mesh(coreGeo, wireMat);
    globeGroup.add(wireMesh);

    // Atmospheric Outer Shadow Mana Glow
    const atmosphereGeo = new THREE.SphereGeometry(GLOBE_RADIUS * 1.15, 32, 32);
    const atmosphereMat = new THREE.MeshBasicMaterial({
      color: 0x9d4edd,
      transparent: true,
      opacity: 0.08,
      side: THREE.BackSide,
    });
    const atmosphereMesh = new THREE.Mesh(atmosphereGeo, atmosphereMat);
    globeGroup.add(atmosphereMesh);

    // Continental lat/lon reference rings
    for (let r = -60; r <= 60; r += 30) {
      const ringLatRad = (r * Math.PI) / 180;
      const ringR = GLOBE_RADIUS * Math.cos(ringLatRad);
      const ringY = GLOBE_RADIUS * Math.sin(ringLatRad);
      const ringGeo = new THREE.BufferGeometry();
      const points: THREE.Vector3[] = [];
      for (let a = 0; a <= 64; a++) {
        const theta = (a / 64) * Math.PI * 2;
        points.push(new THREE.Vector3(ringR * Math.cos(theta), ringY, ringR * Math.sin(theta)));
      }
      ringGeo.setFromPoints(points);
      const ringLine = new THREE.Line(
        ringGeo,
        new THREE.LineBasicMaterial({ color: 0x1e3a5f, transparent: true, opacity: 0.25 })
      );
      globeGroup.add(ringLine);
    }

    // 7. Add Regional Beacons & Pins
    const nodeCoords: { [key in RegionId]: THREE.Vector3 } = {
      EU_WEST: latLongToVector3(nodes.EU_WEST.lat, nodes.EU_WEST.lng, GLOBE_RADIUS),
      US_EAST: latLongToVector3(nodes.US_EAST.lat, nodes.US_EAST.lng, GLOBE_RADIUS),
      AP_SOUTH: latLongToVector3(nodes.AP_SOUTH.lat, nodes.AP_SOUTH.lng, GLOBE_RADIUS),
    };

    Object.entries(nodeCoords).forEach(([regId, pos]) => {
      const isEu = regId === 'EU_WEST';
      // Solo Leveling Palette: EU_WEST (Shadow Violet), US_EAST (System Blue), AP_SOUTH (S-Rank Gold)
      const nodeColor = isEu ? 0xc77dff : regId === 'US_EAST' ? 0x3a86ff : 0xffb703;

      // Vertical beacon pin
      const pinLength = 1.6;
      const pinGeo = new THREE.CylinderGeometry(0.08, 0.08, pinLength, 16);
      const pinMat = new THREE.MeshBasicMaterial({ color: nodeColor });
      const pinMesh = new THREE.Mesh(pinGeo, pinMat);

      // Align pin to point outwards along normal
      const normal = pos.clone().normalize();
      pinMesh.position.copy(pos.clone().add(normal.clone().multiplyScalar(pinLength / 2)));
      pinMesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), normal);
      globeGroup.add(pinMesh);

      // Pulsating surface disk
      const diskGeo = new THREE.RingGeometry(0.3, 0.75, 24);
      const diskMat = new THREE.MeshBasicMaterial({
        color: nodeColor,
        side: THREE.DoubleSide,
        transparent: true,
        opacity: 0.8,
      });
      const diskMesh = new THREE.Mesh(diskGeo, diskMat);
      diskMesh.position.copy(pos.clone().add(normal.clone().multiplyScalar(0.05)));
      diskMesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), normal);
      globeGroup.add(diskMesh);

      // Beacon head sphere
      const headGeo = new THREE.SphereGeometry(0.28, 16, 16);
      const headMat = new THREE.MeshStandardMaterial({
        color: nodeColor,
        emissive: nodeColor,
        emissiveIntensity: 0.9,
      });
      const headMesh = new THREE.Mesh(headGeo, headMat);
      headMesh.position.copy(pos.clone().add(normal.clone().multiplyScalar(pinLength)));
      globeGroup.add(headMesh);

      // Sovereignty Shield dome for EU_WEST (Electric Violet Shadow Flame Aura)
      if (isEu) {
        const shieldGeo = new THREE.SphereGeometry(1.4, 24, 24, 0, Math.PI * 2, 0, Math.PI * 0.5);
        const shieldMat = new THREE.MeshBasicMaterial({
          color: 0x9d4edd,
          wireframe: true,
          transparent: true,
          opacity: 0.45,
        });
        const shieldMesh = new THREE.Mesh(shieldGeo, shieldMat);
        shieldMesh.position.copy(pos);
        shieldMesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), normal);
        globeGroup.add(shieldMesh);

        // 3D Animated Epoch Lease Orbit Halo (INV-7 CAS Quorum Lease Ring)
        const leaseHaloGeo = new THREE.RingGeometry(1.65, 1.85, 32);
        const leaseHaloMat = new THREE.MeshBasicMaterial({
          color: 0x00f5d4,
          side: THREE.DoubleSide,
          transparent: true,
          opacity: 0.85,
        });
        const leaseHaloMesh = new THREE.Mesh(leaseHaloGeo, leaseHaloMat);
        leaseHaloMesh.position.copy(pos.clone().add(normal.clone().multiplyScalar(0.08)));
        leaseHaloMesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), normal);
        globeGroup.add(leaseHaloMesh);
        leaseHaloRef.current = leaseHaloMesh;
      }
    });

    // 8. Generate WAN Inter-Region Arcs (Solo Leveling Shadow Mana & System Routes)
    const linkPairs = [
      { id: 'eu-us', v1: nodeCoords.EU_WEST, v2: nodeCoords.US_EAST, elevation: 0.35, color: 0x9d4edd },
      { id: 'eu-ap', v1: nodeCoords.EU_WEST, v2: nodeCoords.AP_SOUTH, elevation: 0.28, color: 0x7b2cbf },
      { id: 'ap-us', v1: nodeCoords.AP_SOUTH, v2: nodeCoords.US_EAST, elevation: 0.45, color: 0x00f5d4 },
    ];

    linkPairs.forEach(({ id, v1, v2, elevation, color }) => {
      const curve = createElevatedCurve(v1, v2, elevation);
      const points = curve.getPoints(50);
      const curveGeo = new THREE.BufferGeometry().setFromPoints(points);
      const curveMat = new THREE.LineBasicMaterial({
        color,
        transparent: true,
        opacity: 0.65,
        linewidth: 2,
      });
      const line = new THREE.Line(curveGeo, curveMat);
      globeGroup.add(line);
      curvesRef.current[id] = { curve, line };

      // Add animated traffic particles on this arc
      const particleCount = 4;
      for (let p = 0; p < particleCount; p++) {
        const pGeo = new THREE.SphereGeometry(0.12, 12, 12);
        const pMat = new THREE.MeshBasicMaterial({ color: 0xffffff });
        const pMesh = new THREE.Mesh(pGeo, pMat);
        globeGroup.add(pMesh);

        arcParticlesRef.current.push({
          curve,
          mesh: pMesh,
          t: p / particleCount,
          speed: 0.005,
          linkId: id,
        });
      }
    });

    // 9. High-Precision Pointer & Orbit Controls
    // 9. High-Precision Window-Wide Pointer & Orbit Controls
    const canvas = renderer.domElement;
    canvas.style.touchAction = 'none';

    const onPointerDown = (e: PointerEvent) => {
      isDraggingRef.current = true;
      targetRotationRef.current = null; // Clear auto-focus when user manually drags
      previousPositionRef.current = { x: e.clientX, y: e.clientY };
    };

    const onGlobalPointerDown = (e: PointerEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest('button, a, input, select, textarea, [role="button"], .pointer-events-auto')) {
        if (target !== canvas && !target?.closest('.cursor-grab')) return;
      }
      isDraggingRef.current = true;
      targetRotationRef.current = null;
      previousPositionRef.current = { x: e.clientX, y: e.clientY };
    };

    const onPointerMove = (e: PointerEvent) => {
      if (!isDraggingRef.current || !globeGroupRef.current) return;
      const deltaX = e.clientX - previousPositionRef.current.x;
      const deltaY = e.clientY - previousPositionRef.current.y;

      globeGroupRef.current.rotation.y += deltaX * 0.007;
      globeGroupRef.current.rotation.x = Math.max(
        -1.3,
        Math.min(1.3, globeGroupRef.current.rotation.x + deltaY * 0.007)
      );

      previousPositionRef.current = { x: e.clientX, y: e.clientY };
    };

    const onPointerUp = () => {
      isDraggingRef.current = false;
    };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      if (!cameraRef.current) return;
      cameraRef.current.position.z = Math.max(
        12,
        Math.min(42, cameraRef.current.position.z + e.deltaY * 0.025)
      );
    };

    canvas.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('pointerdown', onGlobalPointerDown);
    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('pointercancel', onPointerUp);
    canvas.addEventListener('wheel', onWheel, { passive: false });

    // Initial globe orientation to highlight EU_WEST and US_EAST
    globeGroup.rotation.y = 1.1;
    globeGroup.rotation.x = 0.25;

    // 10. Animation Loop
    const animate = () => {
      animationFrameRef.current = requestAnimationFrame(animate);

      // Starfield subtle rotation
      starField.rotation.y += 0.0003;

      // Smooth interpolation towards target region orientation if set
      if (targetRotationRef.current && globeGroupRef.current && !isDraggingRef.current) {
        globeGroupRef.current.rotation.x += (targetRotationRef.current.x - globeGroupRef.current.rotation.x) * 0.08;
        globeGroupRef.current.rotation.y += (targetRotationRef.current.y - globeGroupRef.current.rotation.y) * 0.08;
        if (
          Math.abs(targetRotationRef.current.x - globeGroupRef.current.rotation.x) < 0.005 &&
          Math.abs(targetRotationRef.current.y - globeGroupRef.current.rotation.y) < 0.005
        ) {
          targetRotationRef.current = null;
        }
      } else if (globeGroupRef.current && autoRotateRef.current && !isDraggingRef.current) {
        // Auto rotation of globe
        globeGroupRef.current.rotation.y += 0.0018;
      }

      // Animate particles along arcs
      arcParticlesRef.current.forEach((item) => {
        item.t += item.speed;
        if (item.t > 1) item.t = 0;

        const pos = item.curve.getPoint(item.t);
        item.mesh.position.copy(pos);

        // Dynamic scale & opacity pulse
        const scale = 0.8 + Math.sin(Date.now() * 0.008 + item.t * 10) * 0.4;
        item.mesh.scale.set(scale, scale, scale);
      });

      // Animate 3D Epoch Lease Orbit Halo (spin + pulse with heartbeat)
      if (leaseHaloRef.current) {
        leaseHaloRef.current.rotation.z += 0.025;
        const pulse = 1.0 + Math.sin(Date.now() * 0.005) * 0.12;
        leaseHaloRef.current.scale.set(pulse, pulse, pulse);
      }

      renderer.render(scene, camera);
    };

    animate();

    // Resize handler
    const handleResize = () => {
      if (!containerRef.current || !rendererRef.current || !cameraRef.current) return;
      const newWidth = isGlobalBackground ? window.innerWidth : (containerRef.current.clientWidth || 450);
      const newHeight = isGlobalBackground ? window.innerHeight : (containerRef.current.clientHeight || 450);
      cameraRef.current.aspect = newWidth / newHeight;
      cameraRef.current.updateProjectionMatrix();
      rendererRef.current.setSize(newWidth, newHeight);
    };

    window.addEventListener('resize', handleResize);

    return () => {
      if (animationFrameRef.current) cancelAnimationFrame(animationFrameRef.current);
      canvas.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('pointerdown', onGlobalPointerDown);
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerUp);
      canvas.removeEventListener('wheel', onWheel);
      window.removeEventListener('resize', handleResize);
      renderer.dispose();
    };
  }, []);

  // Dynamically update Epoch Lease Halo color as lease countdown ticks
  useEffect(() => {
    if (!leaseHaloRef.current) return;
    const mat = leaseHaloRef.current.material as THREE.MeshBasicMaterial;
    if (nodes.EU_WEST.leaseExpiresInSec <= 1) {
      mat.color.setHex(0xd90429); // Crimson: Near lease expiration / fencing danger
      mat.opacity = 0.95;
    } else if (nodes.EU_WEST.leaseExpiresInSec <= 3) {
      mat.color.setHex(0xffb703); // Amber: Heartbeat renewal pending
      mat.opacity = 0.9;
    } else {
      mat.color.setHex(isShadowMonarchMode ? 0xc77dff : 0x00f5d4); // Cyan / Violet: Quorum valid
      mat.opacity = 0.8;
    }
  }, [nodes.EU_WEST.leaseExpiresInSec, isShadowMonarchMode]);

  // Dynamically update link visuals and particle speeds when chaos / WAN conditions change
  useEffect(() => {
    const euUsCurve = curvesRef.current['eu-us'];
    if (euUsCurve) {
      const lineMat = euUsCurve.line.material as THREE.LineBasicMaterial;
      if (isPartitioned) {
        lineMat.color.setHex(0xd90429); // Calamity Blood Crimson for severed
        lineMat.opacity = 0.35;
      } else if (wanDelayMs > 150) {
        lineMat.color.setHex(0xff4d6d); // Kamish Dragon Ember for degraded
        lineMat.opacity = 0.95;
      } else {
        lineMat.color.setHex(0xc77dff); // Solo Leveling Electric Shadow Mana
        lineMat.opacity = 0.85;
      }
    }

    // Update relay lines if adaptive routing is active
    const euApCurve = curvesRef.current['eu-ap'];
    const apUsCurve = curvesRef.current['ap-us'];
    if (euApCurve && apUsCurve) {
      const isRelay = adaptiveRoutingActive && (isPartitioned || wanDelayMs > 150);
      const c1 = euApCurve.line.material as THREE.LineBasicMaterial;
      const c2 = apUsCurve.line.material as THREE.LineBasicMaterial;
      if (isRelay) {
        c1.color.setHex(0x00f5d4); // System Cyan reroute
        c2.color.setHex(0x00f5d4);
        c1.opacity = 0.95;
        c2.opacity = 0.95;
      } else {
        c1.color.setHex(0x7b2cbf); // Shadow Mana Purple
        c2.color.setHex(0x5a189a);
        c1.opacity = 0.5;
        c2.opacity = 0.5;
      }
    }

    // Adjust particle speeds and visibility based on link health
    arcParticlesRef.current.forEach((p) => {
      if (p.linkId === 'eu-us') {
        if (isPartitioned) {
          p.mesh.visible = false;
        } else {
          p.mesh.visible = true;
          const delayFactor = Math.max(1, wanDelayMs / 50);
          p.speed = 0.005 / delayFactor;
          const mat = p.mesh.material as THREE.MeshBasicMaterial;
          mat.color.setHex(
            wanDelayMs > 150 ? 0xff4d6d : 0xe0aaff
          );
        }
      }
      if (p.linkId === 'eu-ap' || p.linkId === 'ap-us') {
        const isRelay = adaptiveRoutingActive && (isPartitioned || wanDelayMs > 150);
        if (isRelay) {
          p.speed = 0.008; // accelerated shadow relay
          const mat = p.mesh.material as THREE.MeshBasicMaterial;
          mat.color.setHex(0x00f5d4);
        } else {
          p.speed = 0.004;
          const mat = p.mesh.material as THREE.MeshBasicMaterial;
          mat.color.setHex(0x9d4edd);
        }
      }
    });
  }, [wanDelayMs, isPartitioned, adaptiveRoutingActive, isShadowMonarchMode]);

  const currentNode = nodes[selectedRegion];

  return (
    <div className={isGlobalBackground ? "fixed inset-0 w-full h-full pointer-events-none z-0 overflow-hidden select-none" : "topology-card relative overflow-hidden flex flex-col h-full"}>
      {/* 3D WebGL Canvas Container - Catches pointer events for free rotation & zoom */}
      <div
        ref={containerRef}
        className={isGlobalBackground ? "w-full h-full cursor-grab active:cursor-grabbing pointer-events-auto" : "relative flex-1 min-h-[380px] w-full cursor-grab active:cursor-grabbing bg-cyber-deep"}
      />

      {/* Floating HUD controls active on 'overview' tab */}
      {activeTab === 'overview' && (
        <>
          {showHud ? (
            <>
              {/* Minimalist 3D Top Floating HUD Bar */}
              <div className="absolute top-16 left-4 right-4 flex flex-wrap items-center justify-between gap-2 px-3.5 py-1.5 rounded-xl border border-white/5 bg-[#080512]/80 backdrop-blur-md pointer-events-auto text-xs font-mono shadow-xl z-20">
                <div className="flex items-center space-x-2">
                  <Globe className="w-3.5 h-3.5 text-purple-400" />
                  <span className="font-semibold tracking-wider text-slate-200 uppercase text-xs">
                    Global 3D WAN Mesh
                  </span>
                  <span className="text-[10px] text-slate-500 font-normal">WebGL Topology</span>
                </div>

                <div className="flex items-center space-x-2">
                  {/* Auto rotate toggle */}
                  <button
                    onClick={() => setAutoRotate(!autoRotate)}
                    className={`text-[11px] flex items-center gap-1 px-2.5 py-0.5 rounded transition-colors ${
                      autoRotate
                        ? 'bg-purple-600/30 text-purple-300 border border-purple-500/40'
                        : 'text-slate-400 bg-black/40 border border-white/5'
                    }`}
                    title="Toggle Earth Auto-Rotation"
                  >
                    <RotateCw className={`w-3 h-3 ${autoRotate ? 'animate-spin' : ''}`} />
                    <span>{autoRotate ? 'Orbiting' : 'Paused'}</span>
                  </button>

                  {/* Region Selector Pills */}
                  <div className="flex items-center bg-black/50 p-0.5 rounded-lg border border-white/5 space-x-1">
                    {(['EU_WEST', 'US_EAST', 'AP_SOUTH'] as RegionId[]).map((r) => {
                      const isSel = selectedRegion === r;
                      const node = nodes[r];
                      return (
                        <button
                          key={r}
                          onClick={() => setSelectedRegion(r)}
                          className={`text-[11px] font-mono px-2 py-0.5 rounded transition-all flex items-center gap-1.5 ${
                            isSel
                              ? 'bg-white/10 text-white font-bold'
                              : 'text-slate-400 hover:text-slate-200'
                          }`}
                        >
                          <span
                            className={`w-1.5 h-1.5 rounded-full ${
                              node.status === 'HEALTHY'
                                ? 'bg-emerald-400'
                                : node.status === 'DEGRADED'
                                ? 'bg-amber-400'
                                : 'bg-rose-500'
                            }`}
                          />
                          <span>{r}</span>
                        </button>
                      );
                    })}
                  </div>

                  {setShowHud && (
                    <button
                      onClick={() => setShowHud(false)}
                      className="text-[11px] text-slate-400 hover:text-white px-2 py-0.5 rounded bg-black/40 border border-white/5 transition-colors"
                      title="Hide HUD panels for clean 3D exploration"
                    >
                      Hide HUD
                    </button>
                  )}
                </div>
              </div>

              {/* Floating Quick Camera & Orbit Controls HUD (Bottom Right) */}
              <div className="absolute bottom-6 right-4 flex flex-col items-center gap-1.5 bg-[#080512]/85 backdrop-blur-md p-1.5 rounded-lg border border-white/10 shadow-2xl pointer-events-auto font-mono text-xs z-20">
                <div className="text-[9px] text-slate-400 font-bold uppercase tracking-wider mb-0.5">Orbit</div>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => rotateGlobe(-0.35)}
                    className="w-7 h-7 flex items-center justify-center rounded bg-black/50 text-slate-200 hover:text-white hover:bg-purple-600 transition-colors border border-white/10"
                    title="Rotate Left"
                  >
                    ◀
                  </button>
                  <button
                    onClick={() => rotateGlobe(0.35)}
                    className="w-7 h-7 flex items-center justify-center rounded bg-black/50 text-slate-200 hover:text-white hover:bg-purple-600 transition-colors border border-white/10"
                    title="Rotate Right"
                  >
                    ▶
                  </button>
                </div>
                <div className="w-full h-px bg-white/5 my-0.5" />
                <div className="text-[9px] text-slate-400 font-bold uppercase tracking-wider mb-0.5">Zoom</div>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => zoomCamera(-3)}
                    className="w-7 h-7 flex items-center justify-center rounded bg-black/50 text-slate-200 hover:text-white hover:bg-emerald-600 transition-colors border border-white/10 font-bold text-sm"
                    title="Zoom In"
                  >
                    +
                  </button>
                  <button
                    onClick={() => zoomCamera(3)}
                    className="w-7 h-7 flex items-center justify-center rounded bg-black/50 text-slate-200 hover:text-white hover:bg-emerald-600 transition-colors border border-white/10 font-bold text-sm"
                    title="Zoom Out"
                  >
                    -
                  </button>
                </div>
                <div className="w-full h-px bg-white/5 my-0.5" />
                <button
                  onClick={resetView}
                  className="w-full py-1 text-[9px] uppercase font-bold rounded bg-black/50 text-slate-300 hover:text-white hover:bg-white/10 transition-colors border border-white/10 text-center"
                  title="Reset Perspective"
                >
                  Reset
                </button>
              </div>

              {/* Floating Region Inspector Overlay (Top Left) */}
              <div className="absolute top-28 left-4 p-2.5 rounded-xl bg-[#080512]/85 backdrop-blur-md border border-white/10 shadow-2xl pointer-events-auto max-w-[240px] font-mono text-xs z-20">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-1.5">
                    <span className="font-bold text-white text-xs">{currentNode.id}</span>
                    <span className="text-[10px] text-slate-400">({currentNode.city})</span>
                  </div>
                  <span
                    className={`text-[9px] font-bold px-1.5 py-0.2 rounded ${
                      currentNode.status === 'HEALTHY'
                        ? 'bg-emerald-500/15 text-emerald-400'
                        : currentNode.status === 'DEGRADED'
                        ? 'bg-amber-500/15 text-amber-400'
                        : 'bg-rose-500/15 text-rose-400'
                    }`}
                  >
                    {currentNode.status}
                  </span>
                </div>

                <div className="grid grid-cols-2 gap-1.5 text-[10px]">
                  <div className="bg-black/50 p-1.5 rounded border border-white/5">
                    <span className="text-slate-400 block text-[9px]">ROLE</span>
                    <span className="text-purple-300 font-semibold">{currentNode.role} (Ep. {currentNode.activeEpoch})</span>
                  </div>
                  <div className="bg-black/50 p-1.5 rounded border border-white/5">
                    <span className="text-slate-400 block text-[9px]">COMMITTED</span>
                    <span className="text-emerald-400 font-semibold">{currentNode.localCommittedOffset}</span>
                  </div>
                  <div className="bg-black/50 p-1.5 rounded border border-white/5">
                    <span className="text-slate-400 block text-[9px]">WATERMARK</span>
                    <span className="text-indigo-300 font-semibold">{currentNode.durableWatermark}</span>
                  </div>
                  <div className="bg-black/50 p-1.5 rounded border border-white/5">
                    <span className="text-slate-400 block text-[9px]">OUTBOX</span>
                    <span className={currentNode.outboxQueueSize > 0 ? 'text-amber-400 font-semibold' : 'text-slate-400'}>
                      {currentNode.outboxQueueSize} msgs
                    </span>
                  </div>

                  {/* Animated Epoch Lease Countdown Bar */}
                  <div className="bg-black/50 p-1.5 rounded border border-white/5 col-span-2">
                    <div className="flex items-center justify-between text-[9px] text-slate-400 mb-1">
                      <span>EPOCH {currentNode.activeEpoch} CAS LEASE</span>
                      <span
                        className={`font-semibold tabular-nums ${
                          currentNode.leaseExpiresInSec <= 1
                            ? 'text-rose-400'
                            : currentNode.leaseExpiresInSec <= 3
                            ? 'text-amber-400'
                            : 'text-emerald-400'
                        }`}
                      >
                        {currentNode.leaseExpiresInSec.toFixed(1)}s / 10s
                      </span>
                    </div>
                    <div className="w-full bg-black/80 h-1 rounded-full overflow-hidden">
                      <div
                        className="h-full transition-all duration-100 ease-linear rounded-full"
                        style={{
                          width: `${Math.min(100, Math.max(0, (currentNode.leaseExpiresInSec / 10) * 100))}%`,
                          backgroundColor:
                            currentNode.leaseExpiresInSec <= 1
                              ? '#d90429'
                              : currentNode.leaseExpiresInSec <= 3
                              ? '#ffb703'
                              : '#00f5d4',
                        }}
                      />
                    </div>
                  </div>
                </div>

                {currentNode.strictLocalShieldActive && (
                  <div className="mt-2 pt-1.5 border-t border-white/5 flex items-center gap-1.5 text-[10px] text-emerald-400">
                    <Shield className="w-3 h-3 text-emerald-400 shrink-0" />
                    <span>StrictLocal Sovereignty Shield</span>
                  </div>
                )}
              </div>

              {/* Dynamic Status Legend (Bottom Left) */}
              <div className="absolute bottom-6 left-4 flex items-center gap-3 bg-[#080511]/90 backdrop-blur-md px-3 py-1 rounded-full border border-white/10 pointer-events-auto text-[10px] font-mono text-slate-400 shadow-2xl z-20">
                <span className="flex items-center gap-1.5">
                  <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
                  <span>Direct WAN</span>
                </span>
                <span className="flex items-center gap-1.5">
                  <span className="w-1.5 h-1.5 rounded-full bg-amber-400" />
                  <span>Degraded</span>
                </span>
                <span className="flex items-center gap-1.5">
                  <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                  <span>Relay (AP_SOUTH)</span>
                </span>
                <span className="flex items-center gap-1.5">
                  <span className="w-1.5 h-1.5 rounded-full bg-rose-500" />
                  <span>Severed</span>
                </span>

                {adaptiveRoutingActive && (isPartitioned || wanDelayMs > 150) && (
                  <span className="text-emerald-400 flex items-center gap-1 font-semibold pl-2 border-l border-white/10">
                    <Zap className="w-3 h-3 text-emerald-400" />
                    <span>Adaptive Routing Active</span>
                  </span>
                )}
              </div>
            </>
          ) : (
            <div className="absolute bottom-6 right-4 pointer-events-auto z-20">
              <button
                onClick={() => setShowHud && setShowHud(true)}
                className="px-3.5 py-1.5 rounded-full bg-purple-600/90 hover:bg-purple-500 text-white font-mono text-xs font-semibold shadow-2xl transition-colors flex items-center gap-1.5 border border-purple-400/40"
              >
                <Globe className="w-3.5 h-3.5" />
                <span>Show HUD</span>
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
};
