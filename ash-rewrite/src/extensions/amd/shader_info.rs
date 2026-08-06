//! <https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/VK_AMD_shader_info.html>

use crate::read_into_uninitialized_vector;
use crate::vk;
use crate::VkResult;
use alloc::vec::Vec;
use core::mem;
use core::mem::size_of_val; // TODO: Remove when bumping MSRV to 1.80

impl crate::amd::shader_info::Device {
    /// <https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/vkGetShaderInfoAMD.html> with [`vk::ShaderInfoTypeAMD::STATISTICS_AMD`]
    #[inline]
    pub unsafe fn get_shader_info_statistics(
        &self,
        pipeline: vk::Pipeline,
        shader_stage: vk::ShaderStageFlags,
    ) -> VkResult<vk::ShaderStatisticsInfoAMD> {
        let mut info = mem::MaybeUninit::<vk::ShaderStatisticsInfoAMD>::uninit();
        let mut size = size_of_val(&info);
        (self.fp.get_shader_info_amd)(
            self.handle,
            pipeline,
            shader_stage,
            vk::ShaderInfoTypeAMD::STATISTICS_AMD,
            &mut size,
            info.as_mut_ptr().cast(),
        )
        .result()?;
        assert_eq!(size, size_of_val(&info));
        Ok(info.assume_init())
    }

    /// <https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/vkGetShaderInfoAMD.html> with [`vk::ShaderInfoTypeAMD::BINARY_AMD`]
    #[inline]
    pub unsafe fn get_shader_info_binary(
        &self,
        pipeline: vk::Pipeline,
        shader_stage: vk::ShaderStageFlags,
    ) -> VkResult<Vec<u8>> {
        read_into_uninitialized_vector(|count, data: *mut u8| {
            (self.fp.get_shader_info_amd)(
                self.handle,
                pipeline,
                shader_stage,
                vk::ShaderInfoTypeAMD::BINARY_AMD,
                count,
                data.cast(),
            )
        })
    }

    /// <https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/vkGetShaderInfoAMD.html> with [`vk::ShaderInfoTypeAMD::DISASSEMBLY_AMD`]
    #[inline]
    pub unsafe fn get_shader_info_disassembly(
        &self,
        pipeline: vk::Pipeline,
        shader_stage: vk::ShaderStageFlags,
    ) -> VkResult<Vec<u8>> {
        read_into_uninitialized_vector(|count, data: *mut u8| {
            (self.fp.get_shader_info_amd)(
                self.handle,
                pipeline,
                shader_stage,
                vk::ShaderInfoTypeAMD::DISASSEMBLY_AMD,
                count,
                data.cast(),
            )
        })
    }
}
