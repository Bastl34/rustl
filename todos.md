todo:
 * everything should be dynamic
 * TODOs
 * mipmap off by default? it uses a lot of GPU mempry (+1/3)

done:
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