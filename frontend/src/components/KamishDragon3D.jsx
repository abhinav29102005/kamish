import React, { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';

export default function KamishDragon3D({ isFlameOn }) {
  const mountRef = useRef(null);
  const flameRef = useRef(isFlameOn);
  const [loadProgress, setLoadProgress] = useState(0);
  const [isLoaded, setIsLoaded] = useState(false);
  const [isWebGlError, setIsWebGlError] = useState(false);

  useEffect(() => { flameRef.current = isFlameOn; }, [isFlameOn]);

  useEffect(() => {
    const container = mountRef.current;
    if (!container) return;

    const width = container.clientWidth;
    const height = container.clientHeight;

    // 1. Scene Setup
    const scene = new THREE.Scene();

    // 2. Camera Setup (Locked, non-movable by user, positioned for full wingspan)
    const camera = new THREE.PerspectiveCamera(38, width / height, 0.1, 1000);
    camera.position.set(0, 0.05, 3.8);

    // 3. Renderer with HDR & Filmic Tone Mapping with WebGL fallback
    let renderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
      renderer.setSize(width, height);
      renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
      renderer.toneMapping = THREE.ACESFilmicToneMapping;
      renderer.toneMappingExposure = 1.48;
      renderer.outputColorSpace = THREE.SRGBColorSpace;
      container.appendChild(renderer.domElement);
    } catch (err) {
      console.warn("WebGL renderer creation failed:", err);
      setIsWebGlError(true);
      return;
    }

    // 4. Atmospheric Sovereign Lighting
    const ambientLight = new THREE.AmbientLight(0x281245, 2.3);
    scene.add(ambientLight);

    // Key Light: Specular highlights on dragon skull & horns
    const keyLight = new THREE.DirectionalLight(0xfff7ed, 3.8);
    keyLight.position.set(3.5, 4.5, 4.5);
    scene.add(keyLight);

    // Fill Light: Soft violet fill
    const fillLight = new THREE.DirectionalLight(0x9333ea, 2.6);
    fillLight.position.set(-3.5, -1, 3);
    scene.add(fillLight);

    // Rim / Backlight: High intensity for glowing wing margins
    const rimLight = new THREE.DirectionalLight(0xc084fc, 6.2);
    rimLight.position.set(0, 3.5, -4.0);
    scene.add(rimLight);

    // Uplight
    const upLight = new THREE.DirectionalLight(0x581c87, 2.2);
    upLight.position.set(0, -3.5, 1.5);
    scene.add(upLight);

    // Mouth / Flame Point Light
    const mouthLight = new THREE.PointLight(0xa855f7, 0, 18);
    mouthLight.position.set(0, 0.52, 1.15);
    scene.add(mouthLight);

    // 5. Root Group for Dragon (Locked orientation, user cannot move)
    const dragonRoot = new THREE.Group();
    scene.add(dragonRoot);

    let mixer = null;
    let modelMaterials = [];

    // 6. Volumetric Hellfire Particle System
    // Core Flame
    const flameCount = 450;
    const flameGeo = new THREE.BufferGeometry();
    const flamePos = new Float32Array(flameCount * 3);
    const flameVels = [];

    for (let i = 0; i < flameCount; i++) {
      flamePos[i * 3] = 0;
      flamePos[i * 3 + 1] = 0;
      flamePos[i * 3 + 2] = 0;
      flameVels.push({
        x: (Math.random() - 0.5) * 0.42,
        y: -0.1 - Math.random() * 0.32,
        z: 1.2 + Math.random() * 1.8,
        life: Math.random(),
      });
    }

    flameGeo.setAttribute('position', new THREE.BufferAttribute(flamePos, 3));

    const flameMat = new THREE.PointsMaterial({
      size: 0.18,
      color: 0xc084fc,
      transparent: true,
      opacity: 0.0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    const flamePoints = new THREE.Points(flameGeo, flameMat);
    flamePoints.position.set(0, 0.45, 0.7);
    dragonRoot.add(flamePoints);

    // Swirling Embers
    const emberCount = 180;
    const emberGeo = new THREE.BufferGeometry();
    const emberPos = new Float32Array(emberCount * 3);
    const emberVels = [];

    for (let i = 0; i < emberCount; i++) {
      emberPos[i * 3] = (Math.random() - 0.5) * 0.4;
      emberPos[i * 3 + 1] = (Math.random() - 0.5) * 0.4;
      emberPos[i * 3 + 2] = (Math.random() - 0.5) * 0.4;
      emberVels.push({
        x: (Math.random() - 0.5) * 0.7,
        y: (Math.random() - 0.2) * 0.6,
        z: (Math.random() - 0.5) * 0.7,
        life: Math.random(),
      });
    }

    emberGeo.setAttribute('position', new THREE.BufferAttribute(emberPos, 3));

    const emberMat = new THREE.PointsMaterial({
      size: 0.08,
      color: 0xffffff,
      transparent: true,
      opacity: 0.0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    const emberPoints = new THREE.Points(emberGeo, emberMat);
    emberPoints.position.set(0, 0.45, 0.7);
    dragonRoot.add(emberPoints);

    // 7. Load Real Dragon Animated GLB Model (Tuned Flying Speed: Not too slow, not too fast)
    const loader = new GLTFLoader();
    loader.load(
      '/models/kamish_dragon_animated.glb',
      (gltf) => {
        const dragonModel = gltf.scene;

        // Auto-center geometry bounding box
        const box = new THREE.Box3().setFromObject(dragonModel);
        const center = box.getCenter(new THREE.Vector3());
        const size = box.getSize(new THREE.Vector3());

        dragonModel.position.x -= center.x;
        dragonModel.position.y -= center.y;
        dragonModel.position.z -= center.z;

        // Scale to fill viewport gracefully without cropping
        const maxDim = Math.max(size.x, size.y, size.z);
        const desiredScale = 3.35 / maxDim;
        dragonRoot.scale.set(desiredScale, desiredScale, desiredScale);

        // Collect meshes & tune PBR materials
        dragonModel.traverse((child) => {
          if (child.isMesh && child.material) {
            modelMaterials.push(child.material);
            child.material.roughness = Math.min(child.material.roughness || 0.35, 0.45);
            child.material.metalness = Math.max(child.material.metalness || 0.4, 0.65);
            child.castShadow = true;
            child.receiveShadow = true;
          }
        });

        // Initialize Blender Flying Animation with Natural Cadence (~2.6s per wing stroke)
        if (gltf.animations && gltf.animations.length > 0) {
          mixer = new THREE.AnimationMixer(dragonModel);
          const flightAction = mixer.clipAction(gltf.animations[0]);
          flightAction.setEffectiveTimeScale(0.92); // Perfectly balanced: not too slow, not too fast
          flightAction.play();
        }

        dragonRoot.add(dragonModel);
        setIsLoaded(true);
      },
      (xhr) => {
        if (xhr.total > 0) {
          const pct = Math.round((xhr.loaded / xhr.total) * 100);
          setLoadProgress(pct);
        }
      },
      (err) => {
        console.error('Failed to load Kamish dragon animated model:', err);
      }
    );

    // 8. Animation Loop with Harmonized Flight Dynamics
    let clock = new THREE.Clock();
    let animId;

    const animate = () => {
      animId = requestAnimationFrame(animate);
      const delta = clock.getDelta();
      const elapsedTime = clock.getElapsedTime();

      // Play Blender Wing Animation
      if (mixer) {
        mixer.update(delta);
      }

      // Harmonized Aerodynamic Body Lift & Pitch synced with wing stroke
      const flightSpeed = 0.92 * (2 * Math.PI / 3.0); // ~1.93 rad/s (matches ~2.6s wing cycle)
      const liftY = Math.sin(elapsedTime * flightSpeed) * 0.055;
      const pitchX = Math.cos(elapsedTime * flightSpeed) * 0.032;
      const bankedYaw = Math.sin(elapsedTime * (flightSpeed * 0.35)) * 0.05;
      const rollZ = Math.sin(elapsedTime * (flightSpeed * 0.5)) * 0.02;

      dragonRoot.position.y = liftY;
      dragonRoot.rotation.x = pitchX;
      dragonRoot.rotation.y = bankedYaw;
      dragonRoot.rotation.z = rollZ;

      const isFlame = flameRef.current;

      // Dynamic Flame Eruption when FLAME ON is active
      if (isFlame) {
        mouthLight.intensity = 14.0 + Math.sin(elapsedTime * 24.0) * 5.0;
        flameMat.opacity = 0.94;
        emberMat.opacity = 0.88;

        modelMaterials.forEach((m) => {
          m.emissive.setHex(0x3a125c);
          m.emissiveIntensity = 0.55;
        });

        // Core flame particle stream
        const positions = flameGeo.attributes.position.array;
        for (let i = 0; i < flameCount; i++) {
          const vel = flameVels[i];
          positions[i * 3] += vel.x;
          positions[i * 3 + 1] += vel.y + 0.015;
          positions[i * 3 + 2] += vel.z * 0.1;

          vel.life -= 0.035;
          if (vel.life <= 0) {
            positions[i * 3] = (Math.random() - 0.5) * 0.12;
            positions[i * 3 + 1] = (Math.random() - 0.5) * 0.12;
            positions[i * 3 + 2] = 0;
            vel.life = 1.0;
          }
        }
        flameGeo.attributes.position.needsUpdate = true;

        // Ember sparks particle stream
        const ePos = emberGeo.attributes.position.array;
        for (let j = 0; j < emberCount; j++) {
          const ev = emberVels[j];
          ePos[j * 3] += ev.x * 0.08;
          ePos[j * 3 + 1] += ev.y * 0.08;
          ePos[j * 3 + 2] += ev.z * 0.08;

          ev.life -= 0.02;
          if (ev.life <= 0) {
            ePos[j * 3] = (Math.random() - 0.5) * 0.3;
            ePos[j * 3 + 1] = (Math.random() - 0.5) * 0.3;
            ePos[j * 3 + 2] = 0;
            ev.life = 1.0;
          }
        }
        emberGeo.attributes.position.needsUpdate = true;
      } else {
        mouthLight.intensity = THREE.MathUtils.lerp(mouthLight.intensity, 0, 0.1);
        flameMat.opacity = THREE.MathUtils.lerp(flameMat.opacity, 0, 0.1);
        emberMat.opacity = THREE.MathUtils.lerp(emberMat.opacity, 0, 0.1);

        modelMaterials.forEach((m) => {
          m.emissive.setHex(0x19082e);
          m.emissiveIntensity = 0.18;
        });
      }

      renderer.render(scene, camera);
    };

    animate();

    const handleResize = () => {
      if (!container) return;
      const w = container.clientWidth;
      const h = container.clientHeight;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
    };

    window.addEventListener('resize', handleResize);

    return () => {
      cancelAnimationFrame(animId);
      window.removeEventListener('resize', handleResize);
      if (container && renderer.domElement) {
        container.removeChild(renderer.domElement);
      }
      renderer.dispose();
    };
  }, []);

  return (
    <div
      style={{
        position: 'relative',
        width: '100%',
        height: '620px',
        userSelect: 'none',
      }}
    >
      <div
        ref={mountRef}
        style={{
          width: '100%',
          height: '100%',
          cursor: 'default',
        }}
      />

      {/* Loading Overlay */}
      {!isLoaded && (
        <div
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            background: 'radial-gradient(circle, rgba(16,8,32,0.92) 0%, rgba(9,4,18,0.98) 100%)',
            zIndex: 10,
          }}
        >
          <div
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: '1rem',
              color: 'var(--text-bright)',
              letterSpacing: '0.22em',
              marginBottom: '1rem',
              fontWeight: 800,
            }}
          >
            AWAKENING KAMISH... {loadProgress > 0 ? `${loadProgress}%` : ''}
          </div>
          <div
            style={{
              width: '240px',
              height: '6px',
              background: 'rgba(255,255,255,0.1)',
              borderRadius: '3px',
              overflow: 'hidden',
              border: '1px solid var(--border-glow)',
            }}
          >
            <div
              style={{
                width: `${Math.max(loadProgress, 18)}%`,
                height: '100%',
                background: 'linear-gradient(90deg, #a855f7, #c084fc, #ffffff)',
                transition: 'width 0.2s ease',
              }}
            />
          </div>
          <div
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: '0.75rem',
              color: 'var(--text-dim)',
              marginTop: '0.8rem',
              letterSpacing: '0.12em',
            }}
          >
            SOVEREIGN DRAGON · REAL-TIME 3D FLIGHT
          </div>
        </div>
      )}

      {/* Atmospheric Nebula Aura behind the Dragon */}
      <div
        style={{
          position: 'absolute',
          top: '38%',
          left: '50%',
          transform: 'translate(-50%, -50%)',
          width: '560px',
          height: '420px',
          background: 'radial-gradient(ellipse at center, rgba(168, 85, 247, 0.38) 0%, rgba(147, 51, 234, 0.16) 45%, transparent 75%)',
          filter: 'blur(45px)',
          pointerEvents: 'none',
          zIndex: 0,
        }}
      />

      {/* Ground Shadow Nebula */}
      <div
        style={{
          position: 'absolute',
          bottom: '8%',
          left: '50%',
          transform: 'translateX(-50%)',
          width: '420px',
          height: '65px',
          background: 'radial-gradient(ellipse at center, rgba(168, 85, 247, 0.4) 0%, transparent 70%)',
          filter: 'blur(20px)',
          pointerEvents: 'none',
          zIndex: 0,
        }}
      />
    </div>
  );
}
