todo:
 * everything should be dynamic
 * TODOs
 * normal map: rotation/transformation is not applied correctly
 * screenshot is broken on windows
 * flickering on windows (amd)
 * cleanup is not working (clear scene)
 * better scene statistics graph
 * winit + wgpu update
 * deadlock while asset loading
 * update of gamma and exposure somehow encapsulate of "complete" scene settings
 * memory leak
 * dead lock while loading an object/scene (just sometimes)
 * optimize shader - do not use empty morph targets or animation weights
 * action management

done:
 * limit texture resolution for load
 * rework id manager to use arc rwlock (to prevent the need of execute_on_scene_mut_and_wait)
 * get rid of async stuff -> use exec queue
 * set camera resolution per default (otherwise target rotation controller is not working correctly)
 * camera target not working correctly (maybe: get_center wrong?)
 * mipmap off by default? it uses a lot of GPU mempry (+1/3)
 * something is wrong while showing a preview (8k textures) - load time is way to high
 * consider using ComponentBase for all state structs
 * blender like movement (g +xyz, r +xyz)
 * normal matrix is wrong -> its actually this: https://stackoverflow.com/questions/21079623/how-to-calculate-the-normal-matrix
 * active camera (camera width/height based on 0<->1)
 * instance visibility (in rendering)
 * cleanup vertex/instancing structure
 * dynamic light amount
 * view space ligthning (https://sotrh.github.io/learn-wgpu/intermediate/tutorial10-lighting/#the-normal-matrix)
 * get depth map
 * remove extra_color_attachment
 * use BufferDimensions for texture save
 * screenshot (render pass color attachment)
 * update camera/s on resize
 * do not save state on scene (just pass it on update/render)
 * buffer update on model matrix change