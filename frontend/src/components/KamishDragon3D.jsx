import React, { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';

export default function KamishDragon3D({ isFlameOn, isKillMode = false }) {
  const mountRef = useRef(null);
  const flameRef = useRef(isFlameOn || isKillMode);
  const killModeRef = useRef(isKillMode);
  const [loadProgress, setLoadProgress] = useState(0);
  const [isLoaded, setIsLoaded] = useState(false);

  flameRef.current = isFlameOn || isKillMode;
  killModeRef.current = isKillMode;

  useEffect(() => {
    const container = mountRef.current;
    if (!container) return;

    const width = container.clientWidth;
    const height = container.clientHeight;

    // 1. Scene Setup
    const scene = new THREE.Scene();

    // 2. Camera Setup: Tuned FOV and distance for full wingspan visibility with massive presence
    const camera = new THREE.PerspectiveCamera(38, width / height, 0.1, 1000);
    const baseCamPos = new THREE.Vector3(0, 0.04, 3.85);
    camera.position.copy(baseCamPos);

    // 3. Renderer with High Dynamic Range & Tone Mapping
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setSize(width, height);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.45;
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    container.appendChild(renderer.domElement);

    // 4. Cinematic Multi-Point Lighting System
    const ambientLight = new THREE.AmbientLight(0x281245, 2.2);
    scene.add(ambientLight);

    // Key Light
    const keyLight = new THREE.DirectionalLight(0xfff7ed, 3.6);
    keyLight.position.set(3.5, 4.5, 4.5);
    scene.add(keyLight);

    // Fill Light
    const fillLight = new THREE.DirectionalLight(0x9333ea, 2.6);
    fillLight.position.set(-3.5, -1, 3);
    scene.add(fillLight);

    // High-Intensity Rim / Backlight for razor-sharp wing silhouettes
    const rimLight = new THREE.DirectionalLight(0xc084fc, 6.0);
    rimLight.position.set(0, 3.5, -4.0);
    scene.add(rimLight);

    // Uplight
    const upLight = new THREE.DirectionalLight(0x581c87, 2.2);
    upLight.position.set(0, -3.5, 1.5);
    scene.add(upLight);

    // Mouth / Inferno Maw Light
    const mouthLight = new THREE.PointLight(0xa855f7, 0, 18);
    mouthLight.position.set(0, 0.52, 1.15);
    scene.add(mouthLight);

    // 5. Root Group for Dragon
    const dragonRoot = new THREE.Group();
    scene.add(dragonRoot);

    let dragonModel = null;
    let modelMaterials = [];

    // 6. Volumetric Hellfire Particle System
    // Core Flame (500 particles)
    const flameCount = 500;
    const flameGeo = new THREE.BufferGeometry();
    const flamePos = new Float32Array(flameCount * 3);
    const flameVels = [];

    for (let i = 0; i < flameCount; i++) {
      flamePos[i * 3] = 0;
      flamePos[i * 3 + 1] = 0;
      flamePos[i * 3 + 2] = 0;
      flameVels.push({
        x: (Math.random() - 0.5) * 0.45,
        y: -0.1 - Math.random() * 0.35,
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

    // Swirling Fiery Embers & Sparks (200 particles)
    const emberCount = 200;
    const emberGeo = new THREE.BufferGeometry();
    const emberPos = new Float32Array(emberCount * 3);
    const emberVels = [];

    for (let i = 0; i < emberCount; i++) {
      emberPos[i * 3] = (Math.random() - 0.5) * 0.4;
      emberPos[i * 3 + 1] = (Math.random() - 0.5) * 0.4;
      emberPos[i * 3 + 2] = (Math.random() - 0.5) * 0.4;
      emberVels.push({
        x: (Math.random() - 0.5) * 0.75,
        y: (Math.random() - 0.2) * 0.65,
        z: (Math.random() - 0.5) * 0.75,
        life: Math.random(),
      });
    }

    emberGeo.setAttribute('position', new THREE.BufferAttribute(emberPos, 3));

    const emberMat = new THREE.PointsMaterial({
      size: 0.09,
      color: 0xffffff,
      transparent: true,
      opacity: 0.0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    const emberPoints = new THREE.Points(emberGeo, emberMat);
    emberPoints.position.set(0, 0.45, 0.7);
    dragonRoot.add(emberPoints);

    // 7. Load Real Dragon GLB Model
    const loader = new GLTFLoader();
    loader.load(
      '/models/kamish_dragon.glb',
      (gltf) => {
        dragonModel = gltf.scene;

        // Auto-center geometry bounding box
        const box = new THREE.Box3().setFromObject(dragonModel);
        const center = box.getCenter(new THREE.Vector3());
        const size = box.getSize(new THREE.Vector3());

        dragonModel.position.x -= center.x;
        dragonModel.position.y -= center.y;
        dragonModel.position.z -= center.z;

        // Perfectly scaled to fill width without clipping (3.35 unit wingspan)
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
        console.error('Failed to load Kamish dragon model:', err);
      }
    );

    // 8. Interaction: Drag-to-Orbit & Cursor Parallax
    let targetRotY = 0;
    let targetRotX = 0;
    let isDragging = false;
    let prevMouse = { x: 0, y: 0 };
    let mouseParallax = { x: 0, y: 0 };

    const onMouseDown = (e) => {
      isDragging = true;
      prevMouse = { x: e.clientX, y: e.clientY };
    };

    const onMouseMove = (e) => {
      const normX = (e.clientX / window.innerWidth) * 2 - 1;
      const normY = -(e.clientY / window.innerHeight) * 2 + 1;
      mouseParallax.x = normX * 0.28;
      mouseParallax.y = normY * 0.16;

      if (isDragging) {
        const deltaX = e.clientX - prevMouse.x;
        const deltaY = e.clientY - prevMouse.y;
        targetRotY += deltaX * 0.008;
        targetRotX += deltaY * 0.005;
        prevMouse = { x: e.clientX, y: e.clientY };
      }
    };

    const onMouseUp = () => {
      isDragging = false;
    };

    window.addEventListener('mousedown', onMouseDown);
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);

    // Touch support for mobile/tablets
    const onTouchStart = (e) => {
      if (e.touches.length === 1) {
        isDragging = true;
        prevMouse = { x: e.touches[0].clientX, y: e.touches[0].clientY };
      }
    };
    const onTouchMove = (e) => {
      if (isDragging && e.touches.length === 1) {
        const deltaX = e.touches[0].clientX - prevMouse.x;
        const deltaY = e.touches[0].clientY - prevMouse.y;
        targetRotY += deltaX * 0.008;
        targetRotX += deltaY * 0.005;
        prevMouse = { x: e.touches[0].clientX, y: e.touches[0].clientY };
      }
    };
    const onTouchEnd = () => {
      isDragging = false;
    };

    window.addEventListener('touchstart', onTouchStart);
    window.addEventListener('touchmove', onTouchMove);
    window.addEventListener('touchend', onTouchEnd);

    // 9. Main Animation Loop
    let clock = new THREE.Clock();
    let animId;

    const animate = () => {
      animId = requestAnimationFrame(animate);
      const elapsedTime = clock.getElapsedTime();

      const isKill = killModeRef.current;
      const isFlame = flameRef.current;

      // Respiration & Hovering
      const breatheSpeed = isKill ? 2.5 : 1.3;
      const breatheY = Math.sin(elapsedTime * breatheSpeed) * (isKill ? 0.07 : 0.05);
      const breatheTilt = Math.sin(elapsedTime * (breatheSpeed * 0.8)) * 0.025;

      dragonRoot.position.y = breatheY;
      dragonRoot.rotation.z = breatheTilt;

      // Smooth Rotation & Cursor Following
      if (!isDragging) {
        const autoSweep = Math.sin(elapsedTime * 0.4) * 0.12;
        dragonRoot.rotation.y += (targetRotY + autoSweep + mouseParallax.x - dragonRoot.rotation.y) * 0.05;
        dragonRoot.rotation.x += (targetRotX + mouseParallax.y - dragonRoot.rotation.x) * 0.05;
      } else {
        dragonRoot.rotation.y += (targetRotY - dragonRoot.rotation.y) * 0.1;
        dragonRoot.rotation.x += (targetRotX - dragonRoot.rotation.x) * 0.1;
      }

      // 10. KILL MODE vs SOVEREIGN SHADOW MODE
      if (isKill) {
        // KILL MODE: Ominous apocalyptic blood crimson theme
        ambientLight.color.setHex(0x1a0408);
        ambientLight.intensity = 1.6;

        keyLight.color.setHex(0xff1e38);
        keyLight.intensity = 5.2;

        fillLight.color.setHex(0xff5500);
        fillLight.intensity = 3.8;

        rimLight.color.setHex(0xff0044);
        rimLight.intensity = 8.5;

        upLight.color.setHex(0x991b1b);
        upLight.intensity = 3.2;

        mouthLight.color.setHex(0xff2a00);
        mouthLight.intensity = 22.0 + Math.sin(elapsedTime * 30.0) * 7.0;

        flameMat.color.setHex(0xff3300);
        emberMat.color.setHex(0xffdd44);

        // Sinister volcanic scale texture with pulsing magma veins
        modelMaterials.forEach((m) => {
          m.color.setHex(0xff6655);
          m.emissive.setHex(0x8b0000);
          m.emissiveIntensity = 1.0 + Math.sin(elapsedTime * 10.0) * 0.3;
        });

        // Visceral Camera Rumble in Kill Mode
        const shakeMagnitude = 0.007;
        camera.position.x = baseCamPos.x + (Math.random() - 0.5) * shakeMagnitude;
        camera.position.y = baseCamPos.y + (Math.random() - 0.5) * shakeMagnitude;
        camera.position.z = THREE.MathUtils.lerp(camera.position.z, 3.65, 0.05);
      } else {
        // NORMAL SHADOW MONARCH MODE: Deep void obsidian & amethyst
        ambientLight.color.setHex(0x281245);
        ambientLight.intensity = 2.2;

        keyLight.color.setHex(0xfff7ed);
        keyLight.intensity = 3.6;

        fillLight.color.setHex(0x9333ea);
        fillLight.intensity = 2.6;

        rimLight.color.setHex(0xc084fc);
        rimLight.intensity = 6.0;

        upLight.color.setHex(0x581c87);
        upLight.intensity = 2.2;

        mouthLight.color.setHex(0xd8b4fe);
        flameMat.color.setHex(0xc084fc);
        emberMat.color.setHex(0xf3e8ff);

        modelMaterials.forEach((m) => {
          m.color.setHex(0xffffff);
          m.emissive.setHex(0x19082e);
          m.emissiveIntensity = isFlame ? 0.6 : 0.15;
        });

        camera.position.x = THREE.MathUtils.lerp(camera.position.x, baseCamPos.x, 0.1);
        camera.position.y = THREE.MathUtils.lerp(camera.position.y, baseCamPos.y, 0.1);
        camera.position.z = THREE.MathUtils.lerp(camera.position.z, baseCamPos.z, 0.08);
      }

      // Flame Eruption Animation
      if (isFlame || isKill) {
        if (!isKill) {
          mouthLight.intensity = 11.0 + Math.sin(elapsedTime * 22.0) * 4.5;
        }
        flameMat.opacity = isKill ? 0.96 : 0.9;
        emberMat.opacity = isKill ? 0.92 : 0.8;

        // Core flame particle simulation
        const positions = flameGeo.attributes.position.array;
        for (let i = 0; i < flameCount; i++) {
          const vel = flameVels[i];
          positions[i * 3] += vel.x * (isKill ? 1.2 : 1.0);
          positions[i * 3 + 1] += (vel.y + 0.015) * (isKill ? 1.2 : 1.0);
          positions[i * 3 + 2] += vel.z * (isKill ? 0.11 : 0.09);

          vel.life -= isKill ? 0.042 : 0.034;
          if (vel.life <= 0) {
            positions[i * 3] = (Math.random() - 0.5) * 0.12;
            positions[i * 3 + 1] = (Math.random() - 0.5) * 0.12;
            positions[i * 3 + 2] = 0;
            vel.life = 1.0;
          }
        }
        flameGeo.attributes.position.needsUpdate = true;

        // Ember sparks particle simulation
        const ePos = emberGeo.attributes.position.array;
        for (let j = 0; j < emberCount; j++) {
          const ev = emberVels[j];
          ePos[j * 3] += ev.x * (isKill ? 0.1 : 0.08);
          ePos[j * 3 + 1] += ev.y * (isKill ? 0.1 : 0.08);
          ePos[j * 3 + 2] += ev.z * (isKill ? 0.1 : 0.08);

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
      window.removeEventListener('mousedown', onMouseDown);
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
      window.removeEventListener('touchstart', onTouchStart);
      window.removeEventListener('touchmove', onTouchMove);
      window.removeEventListener('touchend', onTouchEnd);
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
        height: '620px', // Perfectly proportioned height with zero cropping
      }}
    >
      <div
        ref={mountRef}
        style={{
          width: '100%',
          height: '100%',
          cursor: isKillMode ? 'crosshair' : 'grab',
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
                background: isKillMode
                  ? 'linear-gradient(90deg, #ff1e38, #ff6b35, #ffffff)'
                  : 'linear-gradient(90deg, #a855f7, #c084fc, #ffffff)',
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
            REAL 3D ANCIENT DRAGON · PBR TEXTURES
          </div>
        </div>
      )}

      {/* Volumetric Calamity Nebula Aura behind the Dragon */}
      <div
        style={{
          position: 'absolute',
          top: '40%',
          left: '50%',
          transform: 'translate(-50%, -50%)',
          width: isKillMode ? '640px' : '540px',
          height: isKillMode ? '480px' : '400px',
          background: isKillMode
            ? 'radial-gradient(ellipse at center, rgba(255, 30, 56, 0.45) 0%, rgba(185, 28, 28, 0.2) 45%, transparent 75%)'
            : 'radial-gradient(ellipse at center, rgba(168, 85, 247, 0.38) 0%, rgba(147, 51, 234, 0.16) 45%, transparent 75%)',
          filter: 'blur(45px)',
          pointerEvents: 'none',
          transition: 'all 0.5s ease',
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
          width: isKillMode ? '480px' : '420px',
          height: '65px',
          background: isKillMode
            ? 'radial-gradient(ellipse at center, rgba(255, 30, 56, 0.5) 0%, transparent 70%)'
            : 'radial-gradient(ellipse at center, rgba(168, 85, 247, 0.4) 0%, transparent 70%)',
          filter: 'blur(20px)',
          pointerEvents: 'none',
          transition: 'all 0.5s ease',
          zIndex: 0,
        }}
      />
    </div>
  );
}
